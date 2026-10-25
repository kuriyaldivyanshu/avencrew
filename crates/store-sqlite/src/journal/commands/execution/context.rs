//! Historical fixture proofs: immutable exact source, separate bounded model view.
use super::*;
use avencrew_contracts::runtime::context::{fixture_context_view, ContextProofRef};
use serde_json::{json, Value};
pub(super) const PROOF_MEDIA: &str = "application/vnd.avencrew.fixture-context-proof+json";

impl LocalStore {
    pub(super) async fn bounded_proof_bytes(
        &mut self,
        w: &DomainId,
        actor: &DomainId,
        id: &DomainId,
        media: &str,
    ) -> Result<Vec<u8>, TurnError> {
        let scope = self.proof_read_scope();
        let key = (w.clone(), actor.clone(), id.clone());
        if media == PROOF_MEDIA {
            if let Some(bytes) = scope
                .lock()
                .map_err(|_| TurnError::Conflict)?
                .proof_bytes
                .get(&key)
            {
                return Ok(bytes.clone());
            }
        }
        let size: i64 =
            sqlx::query_scalar("SELECT byte_size FROM local_blobs WHERE workspace_id=? AND id=?")
                .bind(w.as_str())
                .bind(id.as_str())
                .fetch_one(&mut self.connection)
                .await?;
        {
            let mut budget = scope.lock().map_err(|_| TurnError::Conflict)?;
            let size = usize::try_from(size).map_err(|_| TurnError::Conflict)?;
            if size > budget.remaining || budget.remaining_reads == 0 {
                budget.exhausted = true;
                return Err(TurnError::ContextBudget);
            }
            budget.remaining -= size;
            budget.remaining_reads -= 1;
        }
        let bytes = if media == PROOF_MEDIA {
            self.read_registered_blob(w, actor, id, media).await?
        } else {
            self.read_execution_source_bytes(w, actor, id, media)
                .await?
        };
        let mut budget = scope.lock().map_err(|_| TurnError::Conflict)?;
        budget.verified.insert(
            key.clone(),
            (media.to_owned(), hex(&digest(&bytes)), bytes.len()),
        );
        if media == PROOF_MEDIA {
            budget.proof_bytes.insert(key, bytes.clone());
        }
        Ok(bytes)
    }
    /// Rebuild an unsealed view only from verified source. Never accepts a
    /// caller's summary or repairs an already registered object.
    pub async fn local_context_proof(
        &mut self,
        w: &DomainId,
        actor: &DomainId,
        turn: &DomainId,
        context_bytes: u32,
    ) -> Result<Value, TurnError> {
        let _proof_scope = self.proof_read_scope();
        if !(1..=1_048_576).contains(&context_bytes) {
            return Err(TurnError::InvalidSeed);
        }
        let verified = self.verify_local_control_history(w, actor, turn).await?;
        let context = self
            .verified_local_execution_context(w, actor, turn, &verified.snapshot, &verified.inputs)
            .await?;
        let seed = self.read_local_turn(w, actor, turn).await?.seed;
        let run = run_row(&mut self.connection, w, &seed.run_spec.run_id, actor).await?;
        let mut groups = Vec::new();
        for h in context["history"].as_array().ok_or(TurnError::Conflict)? {
            let step_id: DomainId =
                serde_json::from_value(h["step_id"].clone()).map_err(|_| TurnError::Conflict)?;
            let step = self.read_local_step(w, actor, turn, &step_id).await?;
            let completion = self.read_local_model_completion(w, actor, &step).await?;
            let mut invocations = Vec::new();
            if let Some(Ok(response)) = &completion {
                // Provider proposal order is authoritative, not invocation UUID order.
                for identity in &response.proposal_identities {
                    if let Some(h) = h["invocations"]
                        .as_array()
                        .ok_or(TurnError::Conflict)?
                        .iter()
                        .find(|h| h["id"] == json!(identity.invocation_id))
                    {
                        let proposal = self
                            .read_local_proposal(w, actor, &identity.invocation_id)
                            .await?;
                        let result = if h["state"] == "result" || h["state"] == "unknown" {
                            Some(
                                self.read_local_tool_result(w, actor, &identity.invocation_id)
                                    .await?,
                            )
                        } else {
                            None
                        };
                        invocations.push(json!({"identity":identity,"state":h["state"],"proposal":proposal,"result":result}));
                    }
                }
            }
            // Failed completions contain diagnostics, not completed eligible
            // work. Keep them exact even when their invocation list is empty.
            let complete = matches!(&completion, Some(Ok(r)) if r.proposal_identities.len() == invocations.len())
                && invocations.iter().all(|v| v["state"] == "result");
            groups.push(json!({"step_id":step_id,"model_call_id":step.model_call_id,"model_state":h["model_state"],"completion":completion,"invocations":invocations,"complete":complete}));
        }
        // Exact event records retain typed dispositions and original receipts;
        // inventory references carry their bytes without inflating model input.
        let events=sqlx::query("SELECT seq,event_type,payload_blob_id FROM local_events WHERE workspace_id=? AND run_id=? ORDER BY seq LIMIT 2049")
            .bind(w.as_str()).bind(seed.run_spec.run_id.as_str()).fetch_all(&mut self.connection).await?;
        if events.len() > 2048 {
            return Err(TurnError::Conflict);
        }
        let mut controls = Vec::new();
        let mut event_members = Vec::new();
        for e in events {
            let id = DomainId::new(e.try_get::<String, _>("payload_blob_id")?)
                .map_err(|_| TurnError::Conflict)?;
            let row = sqlx::query(
                "SELECT media_type,sha256,byte_size FROM local_blobs WHERE workspace_id=? AND id=?",
            )
            .bind(w.as_str())
            .bind(id.as_str())
            .fetch_one(&mut self.connection)
            .await?;
            let media: String = row.try_get("media_type")?;
            let bytes = self.read_execution_bytes(w, actor, &id, &media).await?;
            let kind: String = e.try_get("event_type")?;
            if kind == "execution.controls_applied" {
                let v: Value = serde_json::from_slice(&bytes).map_err(|_| TurnError::Conflict)?;
                controls.push(v);
            }
            event_members.push(json!({"seq":e.try_get::<i64,_>("seq")?,"kind":kind,"id":id,"media_type":media,"sha256":hex(&digest(&bytes)),"byte_size":bytes.len().to_string()}));
        }
        let versions=sqlx::query("SELECT v.id,v.artifact_id,v.version_no,a.current_version_id,v.content_blob_id,v.checks_manifest_id FROM local_artifact_versions v JOIN local_artifacts a ON a.workspace_id=v.workspace_id AND a.id=v.artifact_id WHERE v.workspace_id=? AND a.owner_actor_id=? AND v.artifact_id IN (SELECT artifact_id FROM local_artifact_versions WHERE workspace_id=? AND run_id=?) ORDER BY v.artifact_id,v.version_no LIMIT 129")
            .bind(w.as_str()).bind(actor.as_str()).bind(w.as_str()).bind(seed.run_spec.run_id.as_str()).fetch_all(&mut self.connection).await?;
        if versions.len() > 128 {
            return Err(TurnError::Conflict);
        }
        let mut artifacts = Vec::new();
        for v in versions {
            artifacts.push(json!({"id":v.try_get::<String,_>("id")?,"artifact_id":v.try_get::<String,_>("artifact_id")?,"version_no":v.try_get::<i64,_>("version_no")?,"current_version_id":v.try_get::<String,_>("current_version_id")?,"content_blob_id":v.try_get::<String,_>("content_blob_id")?,"checks_manifest_id":v.try_get::<String,_>("checks_manifest_id")?}));
        }
        Ok(
            json!({"schema_version":"avencrew.fixture-context-proof/1","context_bytes":context_bytes,
            "scope":{"workspace_id":w,"turn_id":turn,"run_id":seed.run_spec.run_id,"attempt_id":seed.lease.attempt_id,"epoch":seed.lease.epoch,"event_start":"1","event_end":run.event.to_string(),"accepted_cut":run.accepted.to_string(),"applied_cut":run.applied.to_string(),"prior_step_ordinal":verified.snapshot.progress.state.steps_started},
            "unresolved":{"invocations":verified.snapshot.unresolved_invocations,"models":verified.snapshot.unresolved_model_calls,"pending_step_id":verified.snapshot.progress.pending_step_id},"context":context,"groups":groups,"controls":controls,"event_members":event_members,"artifacts":artifacts}),
        )
    }

    pub(super) async fn verify_context_proof(
        &mut self,
        w: &DomainId,
        actor: &DomainId,
        turn: &DomainId,
        step: &LocalStep,
    ) -> Result<(), TurnError> {
        let Some(reference) = &step.source_proof else {
            return Ok(());
        };
        // Always recheck identity/retained turn before using scoped evidence.
        let seed = self.read_local_turn(w, actor, turn).await?.seed;
        if step.context.workspace_id != *w
            || step.context.run_id != seed.run_spec.run_id
            || step.context.attempt_id != seed.lease.attempt_id
            || step.context.principal_id != seed.run_spec.acting_principal_id
            || step.context.epoch != seed.lease.epoch
            || step.context.step_id != step.binding.step_id
            || step.lease != seed.lease
            || step.provider_profile_id != seed.run_spec.model_profile_id
            || avencrew_contracts::runtime::content_digest(
                &step.request_bytes().map_err(|_| TurnError::Conflict)?,
            ) != step.request_digest
        {
            return Err(TurnError::Conflict);
        }
        let operation = self.proof_read_scope();
        let key = (
            w.clone(),
            actor.clone(),
            turn.clone(),
            hex(&digest(
                &step.retained_bytes().map_err(|_| TurnError::Conflict)?,
            )),
        );
        if operation
            .lock()
            .map_err(|_| TurnError::Conflict)?
            .completed
            .contains(&key)
        {
            return Ok(());
        }
        let bytes = self
            .bounded_proof_bytes(w, actor, &reference.id, PROOF_MEDIA)
            .await?;
        if hex(&digest(&bytes)) != reference.sha256.as_str()
            || bytes.len() as u64 != reference.byte_size.value()
        {
            return Err(TurnError::Conflict);
        }
        let proof: Value = serde_json::from_slice(&bytes).map_err(|_| TurnError::Conflict)?;
        let scope = &proof["scope"];
        let ordinal: i64 = sqlx::query_scalar(
            "SELECT step_ordinal FROM local_steps WHERE workspace_id=? AND turn_id=? AND id=?",
        )
        .bind(w.as_str())
        .bind(turn.as_str())
        .bind(step.binding.step_id.as_str())
        .fetch_one(&mut self.connection)
        .await?;
        if proof["schema_version"] != "avencrew.fixture-context-proof/1"
            || avencrew_contracts::runtime::record_bytes(&proof).map_err(|_| TurnError::Conflict)?
                != bytes
            || scope["workspace_id"] != json!(w)
            || scope["turn_id"] != json!(turn)
            || scope["run_id"] != json!(seed.run_spec.run_id)
            || scope["attempt_id"] != json!(seed.lease.attempt_id)
            || scope["epoch"] != json!(seed.lease.epoch)
            || scope["prior_step_ordinal"] != json!(ordinal - 1)
            || proof["context"]["seed"] != json!(seed)
            || fixture_context_view(&proof, reference, &step.compatibility_digest)
                .map_err(|_| TurnError::Conflict)?
                != step.request
        {
            return Err(TurnError::Conflict);
        }
        // Bounded flat verification, never recursive read_local_step calls.
        let history = proof["context"]["history"]
            .as_array()
            .ok_or(TurnError::Conflict)?;
        if history.len() != ordinal as usize - 1 || ordinal > 50 {
            return Err(TurnError::Conflict);
        }
        for (n, h) in history.iter().enumerate() {
            let id = h["step_id"].as_str().ok_or(TurnError::Conflict)?;
            let actual: i64 = sqlx::query_scalar(
                "SELECT step_ordinal FROM local_steps WHERE workspace_id=? AND turn_id=? AND id=?",
            )
            .bind(w.as_str())
            .bind(turn.as_str())
            .bind(id)
            .fetch_one(&mut self.connection)
            .await?;
            if actual != n as i64 + 1 {
                return Err(TurnError::Conflict);
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        let members = proof["context"]["members"]
            .as_array()
            .ok_or(TurnError::Conflict)?;
        let events = proof["event_members"]
            .as_array()
            .ok_or(TurnError::Conflict)?;
        if members.len() > 1024 || events.len() > 2048 {
            return Err(TurnError::Conflict);
        }
        for (n, m) in members.iter().chain(events).enumerate() {
            let id: DomainId =
                serde_json::from_value(m["id"].clone()).map_err(|_| TurnError::Conflict)?;
            if id == reference.id
                || id == step.binding.context_manifest_id
                || (n < members.len() && !seen.insert(id.clone()))
            {
                return Err(TurnError::Conflict);
            }
            let media = m["media_type"].as_str().ok_or(TurnError::Conflict)?;
            let scope = self.proof_read_scope();
            let cached = scope
                .lock()
                .map_err(|_| TurnError::Conflict)?
                .verified
                .get(&(w.clone(), actor.clone(), id.clone()))
                .cloned();
            let (actual_media, sha256, size) = if let Some(cached) = cached {
                cached
            } else {
                let source = self.bounded_proof_bytes(w, actor, &id, media).await?;
                let verified = (media.to_owned(), hex(&digest(&source)), source.len());
                scope
                    .lock()
                    .map_err(|_| TurnError::Conflict)?
                    .verified
                    .insert((w.clone(), actor.clone(), id.clone()), verified.clone());
                verified
            };
            if actual_media != media
                || m["sha256"] != sha256
                || m["byte_size"]
                    .as_str()
                    .and_then(|v| v.parse::<usize>().ok())
                    != Some(size)
            {
                return Err(TurnError::Conflict);
            }
        }
        operation
            .lock()
            .map_err(|_| TurnError::Conflict)?
            .completed
            .insert(key);
        Ok(())
    }
}

pub(super) fn proof_reference(id: DomainId, bytes: &[u8]) -> Result<ContextProofRef, TurnError> {
    Ok(ContextProofRef {
        id,
        sha256: avencrew_contracts::runtime::content_digest(bytes),
        byte_size: Counter::new(bytes.len().to_string()).map_err(|_| TurnError::Conflict)?,
    })
}
