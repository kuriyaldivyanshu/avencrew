//! Exact fixture-ledger checkpoints. No live filesystem/process coverage claim.
use super::*;
use avencrew_contracts::{scalars::Digest, wire::TurnOutcome};
use serde_json::{json, Value};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ExecutionMember {
    pub id: DomainId,
    pub sha256: Digest,
    pub byte_size: Counter,
    pub media_type: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ExecutionManifest {
    pub schema_version: String,
    pub workspace_id: DomainId,
    pub turn_id: DomainId,
    pub run_id: DomainId,
    pub attempt_id: DomainId,
    pub epoch: PositiveCounter,
    pub seed_sha256: Digest,
    pub state: TurnState,
    pub pending_step_id: Option<DomainId>,
    pub members: Vec<ExecutionMember>,
}
pub struct LocalExecutionCheckpoint<'a> {
    pub access: LocalExecutionAccess<'a>,
    pub state: &'a TurnState,
    pub checkpoint: &'a DomainId,
    pub manifest: &'a DomainId,
    pub coverage_entry: &'a DomainId,
    pub execution_entry: &'a DomainId,
    pub bundle_blob: &'a DomainId,
    pub event: &'a DomainId,
    pub event_blob: &'a DomainId,
}

impl LocalStore {
    pub(super) async fn read_execution_bytes(
        &mut self,
        w: &DomainId,
        actor: &DomainId,
        id: &DomainId,
        media: &str,
    ) -> Result<Vec<u8>, StoreError> {
        if media == "application/vnd.avencrew.vault-bytes" {
            use std::io::Read;
            let file = self.open_vault_blob(w, actor, id).await?;
            let mut bytes = Vec::new();
            file.take(262145).read_to_end(&mut bytes)?;
            if bytes.len() > 262144 {
                return Err(invalid("execution artifact byte limit"));
            }
            Ok(bytes)
        } else {
            self.read_registered_blob(w, actor, id, media).await
        }
    }

    /// Closed inventory of all committed bytes required to reconstruct this turn.
    pub async fn local_execution_context(
        &mut self,
        w: &DomainId,
        actor: &DomainId,
        turn: &DomainId,
    ) -> Result<Value, TurnError> {
        let _proof_scope = self.proof_read_scope();
        let snap = self.local_execution_snapshot(w, actor, turn).await?;
        let inputs = self.read_local_execution_inputs(w, actor, turn).await?;
        self.verified_local_execution_context(w, actor, turn, &snap, &inputs)
            .await
    }

    pub(super) async fn verified_local_execution_context(
        &mut self,
        w: &DomainId,
        actor: &DomainId,
        turn: &DomainId,
        snap: &LocalExecutionSnapshot,
        inputs: &super::controls::VerifiedInputs,
    ) -> Result<Value, TurnError> {
        let retained = self.read_local_turn(w, actor, turn).await?;
        let rows=sqlx::query("SELECT id,step_ordinal,context_blob_id,tool_catalog_blob_id,model_profile_blob_id FROM local_steps WHERE workspace_id=? AND turn_id=? ORDER BY step_ordinal LIMIT 51")
            .bind(w.as_str()).bind(turn.as_str()).fetch_all(&mut self.connection).await?;
        if rows.len() != snap.progress.state.steps_started as usize {
            return Err(TurnError::Conflict);
        }
        let mut ids = std::collections::BTreeSet::new();
        ids.insert(retained.receipt.spec_blob_id.clone());
        ids.insert(retained.receipt.lease_blob_id.clone());
        let mut history = Vec::new();
        for (n, row) in rows.iter().enumerate() {
            if row.try_get::<i64, _>("step_ordinal")? != n as i64 + 1 {
                return Err(TurnError::Conflict);
            }
            let step_id =
                DomainId::new(row.try_get::<String, _>("id")?).map_err(|_| TurnError::Conflict)?;
            let step = self.read_local_step(w, actor, turn, &step_id).await?;
            let completion = self.read_local_model_completion(w, actor, &step).await?;
            if let Some(proof) = &step.source_proof {
                ids.insert(proof.id.clone());
            }
            for column in [
                "context_blob_id",
                "tool_catalog_blob_id",
                "model_profile_blob_id",
            ] {
                ids.insert(
                    DomainId::new(row.try_get::<String, _>(column)?)
                        .map_err(|_| TurnError::Conflict)?,
                );
            }
            let model=sqlx::query("SELECT state,response_blob_id FROM local_model_calls WHERE workspace_id=? AND id=? AND step_id=?")
                .bind(w.as_str()).bind(step.model_call_id.as_str()).bind(step_id.as_str()).fetch_one(&mut self.connection).await?;
            let response: Option<String> = model.try_get("response_blob_id")?;
            if let Some(id) = &response {
                ids.insert(DomainId::new(id.clone()).map_err(|_| TurnError::Conflict)?);
            }
            let invocation_rows=sqlx::query("SELECT id,state,arguments_blob_id,result_blob_id,policy_receipt_blob_id FROM local_invocations WHERE workspace_id=? AND step_id=? ORDER BY id LIMIT 101")
                .bind(w.as_str()).bind(step_id.as_str()).fetch_all(&mut self.connection).await?;
            if !invocation_rows.is_empty()
                && !matches!(&completion,Some(Ok(result)) if result.response.finish==NormalizedModelResponseFinish::Tools && result.proposal_identities.len()==invocation_rows.len())
            {
                return Err(TurnError::Conflict);
            }
            if invocation_rows.len() > 100 {
                return Err(TurnError::Conflict);
            }
            let mut invocations = Vec::new();
            for invocation in invocation_rows {
                let id = DomainId::new(invocation.try_get::<String, _>("id")?)
                    .map_err(|_| TurnError::Conflict)?;
                let proposal = self.read_local_proposal(w, actor, &id).await?;
                let response = completion
                    .as_ref()
                    .and_then(|r| r.as_ref().ok())
                    .ok_or(TurnError::Conflict)?;
                super::invocations::validate_proposal(&step, response, &proposal)?;
                let state: String = invocation.try_get("state")?;
                for column in [
                    "arguments_blob_id",
                    "result_blob_id",
                    "policy_receipt_blob_id",
                ] {
                    if let Some(id) = invocation.try_get::<Option<String>, _>(column)? {
                        ids.insert(DomainId::new(id).map_err(|_| TurnError::Conflict)?);
                    }
                }
                if invocation
                    .try_get::<Option<String>, _>("result_blob_id")?
                    .is_some()
                {
                    let result = self.read_local_tool_result(w, actor, &id).await?;
                    let unknown = matches!(
                        result,
                        avencrew_contracts::wire::ToolResult::Unknown { .. }
                            | avencrew_contracts::wire::ToolResult::Lost { .. }
                    );
                    if (state == "unknown") != unknown {
                        return Err(TurnError::Conflict);
                    }
                }
                let dispatches=sqlx::query("SELECT id,attempt_id,epoch,state FROM local_dispatches WHERE workspace_id=? AND invocation_id=? LIMIT 2").bind(w.as_str()).bind(id.as_str()).fetch_all(&mut self.connection).await?;
                if state == "proposed" {
                    if !dispatches.is_empty()
                        || invocation
                            .try_get::<Option<String>, _>("policy_receipt_blob_id")?
                            .is_some()
                    {
                        return Err(TurnError::Conflict);
                    }
                } else {
                    if dispatches.len() != 1 {
                        return Err(TurnError::Conflict);
                    }
                    let dispatch = &dispatches[0];
                    let expected = match state.as_str() {
                        "dispatched" => "started",
                        "result" => "result",
                        "unknown" => "unknown",
                        _ => return Err(TurnError::Conflict),
                    };
                    if dispatch.try_get::<String, _>("attempt_id")?
                        != step.lease.attempt_id.as_str()
                        || dispatch.try_get::<i64, _>("epoch")? != step.lease.epoch.value() as i64
                        || dispatch.try_get::<String, _>("state")? != expected
                        || Some(dispatch.try_get::<String, _>("id")?)
                            != invocation.try_get::<Option<String>, _>("policy_receipt_blob_id")?
                    {
                        return Err(TurnError::Conflict);
                    }
                }
                invocations.push(json!({"id":id,"state":state}));
            }
            history.push(json!({"step_id":step_id,"model_call_id":step.model_call_id,"model_state":model.try_get::<String,_>("state")?,"response_blob_id":response,"invocations":invocations}));
        }
        let artifacts=sqlx::query("SELECT v.id,v.artifact_id,v.content_blob_id,m.canonical_blob_id FROM local_artifact_versions v JOIN local_artifacts a ON a.workspace_id=v.workspace_id AND a.id=v.artifact_id JOIN local_manifests m ON m.workspace_id=v.workspace_id AND m.id=v.checks_manifest_id WHERE v.workspace_id=? AND a.owner_actor_id=? AND v.artifact_id IN (SELECT artifact_id FROM local_artifact_versions WHERE workspace_id=? AND run_id=?) ORDER BY v.artifact_id,v.version_no LIMIT 129")
            .bind(w.as_str()).bind(actor.as_str()).bind(w.as_str()).bind(retained.seed.run_spec.run_id.as_str()).fetch_all(&mut self.connection).await?;
        if artifacts.len() > 128 {
            return Err(TurnError::Conflict);
        }
        for row in artifacts {
            let artifact = DomainId::new(row.try_get::<String, _>("artifact_id")?)
                .map_err(|_| TurnError::Conflict)?;
            let version =
                DomainId::new(row.try_get::<String, _>("id")?).map_err(|_| TurnError::Conflict)?;
            self.read_local_artifact(w, actor, &artifact, &version)
                .await?;
            for column in ["content_blob_id", "canonical_blob_id"] {
                ids.insert(
                    DomainId::new(row.try_get::<String, _>(column)?)
                        .map_err(|_| TurnError::Conflict)?,
                );
            }
        }
        let command_blobs:Vec<String>=sqlx::query_scalar("SELECT payload_blob_id FROM local_commands WHERE workspace_id=? AND run_id=? ORDER BY seq LIMIT 1025")
            .bind(w.as_str()).bind(retained.seed.run_spec.run_id.as_str()).fetch_all(&mut self.connection).await?;
        for id in command_blobs {
            ids.insert(DomainId::new(id).map_err(|_| TurnError::Conflict)?);
        }
        if ids.len() > 1024 {
            return Err(TurnError::Conflict);
        }
        let mut members = Vec::new();
        for id in ids {
            let row = sqlx::query(
                "SELECT media_type,sha256,byte_size FROM local_blobs WHERE workspace_id=? AND id=?",
            )
            .bind(w.as_str())
            .bind(id.as_str())
            .fetch_one(&mut self.connection)
            .await?;
            let media: String = row.try_get("media_type")?;
            let bytes = if let Some(bytes) = inputs.bytes.get(&id) {
                bytes.clone()
            } else {
                self.read_execution_bytes(w, actor, &id, &media).await?
            };
            let hash = Digest::new(hex(&digest(&bytes))).map_err(|_| TurnError::Conflict)?;
            if row.try_get::<Vec<u8>, _>("sha256")? != digest(&bytes)
                || row.try_get::<i64, _>("byte_size")? != bytes.len() as i64
            {
                return Err(TurnError::Conflict);
            }
            members.push(ExecutionMember {
                id,
                sha256: hash,
                byte_size: Counter::new(bytes.len().to_string())
                    .map_err(|_| TurnError::Conflict)?,
                media_type: media,
            });
        }
        Ok(
            json!({"seed":retained.seed,"state":snap.progress.state,"accepted_inputs":inputs.commands,"history":history,"members":members}),
        )
    }

    pub(crate) async fn verify_execution_manifest(
        &mut self,
        w: &DomainId,
        actor: &DomainId,
        value: &Value,
        current: bool,
    ) -> Result<(), ControlError> {
        let m: ExecutionManifest =
            serde_json::from_value(value.clone()).map_err(|_| ControlError::Conflict)?;
        let retained = self
            .read_local_turn(w, actor, &m.turn_id)
            .await
            .map_err(|_| ControlError::Conflict)?;
        if m.schema_version != "avencrew.execution-manifest/1"
            || m.workspace_id != *w
            || m.run_id != retained.seed.run_spec.run_id
            || m.attempt_id != retained.seed.lease.attempt_id
            || m.epoch != retained.seed.lease.epoch
            || m.seed_sha256.as_str() != hex(&digest(&canonical(&retained.seed)?))
            || m.members.len() > 1024
        {
            return Err(ControlError::Conflict);
        }
        let mut ids = std::collections::BTreeSet::new();
        for member in &m.members {
            if !ids.insert(&member.id) {
                return Err(ControlError::Conflict);
            }
            let bytes = self
                .read_execution_bytes(w, actor, &member.id, &member.media_type)
                .await?;
            if hex(&digest(&bytes)) != member.sha256.as_str()
                || bytes.len() as u64 != member.byte_size.value()
            {
                return Err(ControlError::Conflict);
            }
        }
        if !ids.contains(&retained.receipt.spec_blob_id)
            || !ids.contains(&retained.receipt.lease_blob_id)
        {
            return Err(ControlError::Conflict);
        }
        if current {
            let context = self
                .local_execution_context(w, actor, &m.turn_id)
                .await
                .map_err(|_| ControlError::Conflict)?;
            if canonical(&context["members"])? != canonical(&m.members)? {
                return Err(ControlError::Conflict);
            }
            let snapshot = self
                .local_execution_snapshot(w, actor, &m.turn_id)
                .await
                .map_err(|_| ControlError::Conflict)?;
            if snapshot.progress.pending_step_id != m.pending_step_id
                || snapshot.progress.state.steps_started != m.state.steps_started
                || snapshot.progress.state.applied_input_cut != m.state.applied_input_cut
                || snapshot.progress.state.progress_digest != m.state.progress_digest
                || snapshot.progress.state.no_progress_steps != m.state.no_progress_steps
                || snapshot.progress.state.active_elapsed_ms > m.state.active_elapsed_ms
            {
                return Err(ControlError::Conflict);
            }
        }
        Ok(())
    }

    pub async fn checkpoint_local_execution(
        &mut self,
        publication: LocalExecutionCheckpoint<'_>,
    ) -> Result<DomainId, TurnError> {
        let LocalExecutionCheckpoint {
            access: a,
            state,
            checkpoint,
            manifest,
            coverage_entry,
            execution_entry,
            bundle_blob,
            event,
            event_blob,
        } = publication;
        let seed = self.authorize_local_execution(&a, true).await?;
        let snap = self
            .local_execution_snapshot(a.workspace, a.actor, a.turn)
            .await?;
        if state.steps_started != snap.progress.state.steps_started
            || state.applied_input_cut != snap.progress.state.applied_input_cut
            || state.no_progress_steps != snap.progress.state.no_progress_steps
            || state.progress_digest != snap.progress.state.progress_digest
        {
            return Err(TurnError::Conflict);
        }
        let context = self
            .local_execution_context(a.workspace, a.actor, a.turn)
            .await?;
        let members: Vec<ExecutionMember> =
            serde_json::from_value(context["members"].clone()).map_err(|_| TurnError::Conflict)?;
        let exact = ExecutionManifest {
            schema_version: "avencrew.execution-manifest/1".into(),
            workspace_id: a.workspace.clone(),
            turn_id: a.turn.clone(),
            run_id: seed.run_spec.run_id.clone(),
            attempt_id: seed.lease.attempt_id.clone(),
            epoch: seed.lease.epoch.clone(),
            seed_sha256: Digest::new(hex(&digest(&canonical(&seed)?)))
                .map_err(|_| TurnError::Conflict)?,
            state: state.clone(),
            pending_step_id: snap.progress.pending_step_id,
            members,
        };
        let task = self
            .read_task_bundle(a.workspace, a.actor, &seed.run_spec.task_id)
            .await?;
        let task_value: Value =
            serde_json::from_slice(&task.canonical_bytes).map_err(|_| TurnError::Conflict)?;
        let resource = task_value["records"]
            .as_array()
            .and_then(|rs| rs.iter().find(|r| r["table"] == "tasks"))
            .map(|r| r["record"]["resource_id"].clone())
            .ok_or(TurnError::Conflict)?;
        let row = run_row(
            &mut self.connection,
            a.workspace,
            &seed.run_spec.run_id,
            a.actor,
        )
        .await?;
        let build: String = sqlx::query_scalar(
            "SELECT harness_build_digest FROM local_attempts WHERE workspace_id=? AND id=?",
        )
        .bind(a.workspace.as_str())
        .bind(seed.lease.attempt_id.as_str())
        .fetch_one(&mut self.connection)
        .await?;
        let entry = |id: &DomainId, key: &str, metadata: Value| json!({"id":id,"workspace_id":a.workspace,"created_at":a.now,"manifest_id":manifest,"entry_key":key,"kind":"metadata","blob_id":null,"resource_version_id":null,"invocation_id":null,"process_id":null,"child_run_id":null,"credential_reference_id":null,"sha256":null,"byte_size":null,"mode_bits":null,"disposition":"declared","metadata":metadata});
        let entries = vec![
            entry(
                coverage_entry,
                "coverage",
                json!({"schema_version":"avencrew.checkpoint-coverage/1","capture":"none","filesystem_event_seq":row.event.to_string(),"gaps":["Fixture profile has no live filesystem or process capture"],"context":"exact_fixture_ledger","effects":"fixture_reads_only","processes":"none","attempt_epoch":seed.lease.epoch,"harness_build_digest":build,"protocol_version":1}),
            ),
            entry(
                execution_entry,
                "execution",
                serde_json::to_value(exact).map_err(|_| TurnError::Conflict)?,
            ),
        ];
        let m = json!({"id":manifest,"workspace_id":a.workspace,"created_at":a.now,"owner_resource_id":resource,"kind":"checkpoint","schema_version":1,"digest":hex(&digest(&canonical(&entries)?)),"sealed_at":a.now,"deletion_generation":"0"});
        let mut c = json!({"id":checkpoint,"workspace_id":a.workspace,"created_at":a.now,"run_id":seed.run_spec.run_id,"attempt_id":seed.lease.attempt_id,"event_seq":row.event.to_string(),"accepted_command_seq":row.accepted.to_string(),"applied_command_seq":row.applied.to_string(),"manifest_id":manifest,"context_manifest_id":null,"checkpoint_kind":"logical","schema_version":1,"sealed_at":a.now});
        c["digest"] = json!(hex(&digest(&canonical(
            &json!({"domain":"avencrew.local-checkpoint-record/1","record":c,"manifest_digest":m["digest"]})
        )?)));
        let wrap = |table: &str, r: Value| json!({"table":table,"id":r["id"],"record_schema":if table=="checkpoints"{"server-v1:S0"}else{"server-v1:target"},"record":r});
        let mut records = vec![wrap("checkpoints", c), wrap("manifests", m)];
        records.extend(entries.into_iter().map(|e| wrap("manifest_entries", e)));
        let bytes = canonical(
            &json!({"schema_version":"1.0","purpose":"checkpoint","root":{"table":"checkpoints","id":checkpoint},"records":records,"referenced_blobs":[]}),
        )?;
        self.publish_checkpoint(CheckpointPublication {
            workspace: a.workspace,
            actor: a.actor,
            bytes: &bytes,
            sha256: digest(&bytes),
            bundle_blob,
            event,
            event_blob,
            expected_version: &positive(row.version)?,
        })
        .await?;
        Ok(checkpoint.clone())
    }

    pub async fn commit_local_outcome(
        &mut self,
        a: LocalExecutionAccess<'_>,
        state: &TurnState,
        outcome: &TurnOutcome,
        event: &DomainId,
    ) -> Result<(), TurnError> {
        let snap = self
            .local_execution_snapshot(a.workspace, a.actor, a.turn)
            .await?;
        if let Some(old) = snap.progress.outcome {
            return if old == *outcome {
                Ok(())
            } else {
                Err(TurnError::IdempotencyMismatch)
            };
        }
        let seed = self.authorize_local_execution(&a, true).await?;
        outcome.validate().map_err(|_| TurnError::InvalidSeed)?;
        let checkpoint = match outcome {
            TurnOutcome::Yielded { checkpoint_id, .. }
            | TurnOutcome::Paused { checkpoint_id, .. }
            | TurnOutcome::CandidateResult { checkpoint_id, .. } => checkpoint_id,
            TurnOutcome::Cancelled { checkpoint_id, .. }
            | TurnOutcome::Failed { checkpoint_id, .. } => {
                checkpoint_id.0.as_ref().ok_or(TurnError::Conflict)?
            }
            _ => return Err(TurnError::Unsupported),
        };
        let restored = self
            .read_checkpoint(a.workspace, a.actor, &seed.run_spec.run_id, checkpoint)
            .await?;
        let exact: ExecutionManifest =
            serde_json::from_value(restored.execution.ok_or(TurnError::Conflict)?)
                .map_err(|_| TurnError::Conflict)?;
        if exact.state != *state {
            return Err(TurnError::Conflict);
        }
        let row = run_row(
            &mut self.connection,
            a.workspace,
            &seed.run_spec.run_id,
            a.actor,
        )
        .await?;
        if row.accepted != state.applied_input_cut.value() as i64 {
            return Err(TurnError::StaleAuthority);
        }
        let unresolved: std::collections::BTreeSet<_> = snap
            .unresolved_invocations
            .iter()
            .chain(&snap.unresolved_model_calls)
            .cloned()
            .collect();
        if let TurnOutcome::Cancelled {
            cleanup_refs,
            unresolved_receipt_refs,
            ..
        } = outcome
        {
            if !cleanup_refs.as_slice().is_empty()
                || unresolved_receipt_refs.as_slice().len() != unresolved.len()
                || unresolved_receipt_refs
                    .as_slice()
                    .iter()
                    .cloned()
                    .collect::<std::collections::BTreeSet<_>>()
                    != unresolved
            {
                return Err(TurnError::Conflict);
            }
        }
        if let TurnOutcome::CandidateResult {
            artifact_refs,
            check_result_refs,
            ..
        } = outcome
        {
            if artifact_refs.as_slice().is_empty() {
                return Err(TurnError::Conflict);
            }
            let mut results = std::collections::BTreeSet::new();
            for artifact in artifact_refs.as_slice() {
                let saved = self
                    .read_local_artifact(
                        a.workspace,
                        a.actor,
                        &artifact.artifact_id,
                        &artifact.version_id,
                    )
                    .await?;
                if saved.receipt.artifact != *artifact {
                    return Err(TurnError::Conflict);
                }
                let current: String = sqlx::query_scalar(
                    "SELECT current_version_id FROM local_artifacts WHERE workspace_id=? AND id=?",
                )
                .bind(a.workspace.as_str())
                .bind(artifact.artifact_id.as_str())
                .fetch_one(&mut self.connection)
                .await?;
                if current != artifact.version_id.as_str() {
                    return Err(TurnError::Conflict);
                }
                results.insert(saved.receipt.check_result_id);
            }
            if results != check_result_refs.as_slice().iter().cloned().collect() {
                return Err(TurnError::Conflict);
            }
        }
        let status = match outcome {
            TurnOutcome::Cancelled { .. } if row.cancelled => {
                if unresolved.is_empty() {
                    "cancelled"
                } else {
                    "blocked"
                }
            }
            TurnOutcome::CandidateResult {
                objective_version_id,
                ..
            } if !row.cancelled && *objective_version_id == seed.run_spec.objective_version_id => {
                "verifying"
            }
            TurnOutcome::Yielded { .. } | TurnOutcome::Paused { .. } if !row.cancelled => "paused",
            TurnOutcome::Failed { .. } => "blocked",
            _ => return Err(TurnError::Cancelled),
        };
        let progress = LocalTurnProgress {
            state: state.clone(),
            pending_step_id: None,
            checkpoint_id: Some(checkpoint.clone()),
            outcome: Some(outcome.clone()),
        };
        let bytes = progress_bytes(a.turn, snap.version + 1, &progress)?;
        self.put_canonical(a.workspace, event, &bytes)?;
        let identity = self.verified_identity(a.workspace, a.actor).await?;
        let mut tx = self.connection.begin().await?;
        identity.check_live(&mut tx).await?;
        let live = run_row(&mut tx, a.workspace, &seed.run_spec.run_id, a.actor).await?;
        if live.version != row.version
            || live.cancelled != row.cancelled
            || live.accepted != row.accepted
            || live.epoch != row.epoch
        {
            return Err(TurnError::StaleAuthority);
        }
        let current: Option<String> = sqlx::query_scalar(
            "SELECT current_checkpoint_id FROM local_runs WHERE workspace_id=? AND id=?",
        )
        .bind(a.workspace.as_str())
        .bind(seed.run_spec.run_id.as_str())
        .fetch_one(&mut *tx)
        .await?;
        if current.as_deref() != Some(checkpoint.as_str()) {
            return Err(TurnError::Conflict);
        }
        // Entering the turn is a coordinator transition, independent of inference.
        if row.status == "preparing" {
            sqlx::query("UPDATE local_runs SET status='running' WHERE workspace_id=? AND id=?")
                .bind(a.workspace.as_str())
                .bind(seed.run_spec.run_id.as_str())
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("UPDATE local_runs SET status=? WHERE workspace_id=? AND id=?")
            .bind(status)
            .bind(a.workspace.as_str())
            .bind(seed.run_spec.run_id.as_str())
            .execute(&mut *tx)
            .await?;
        // Unknown dispatched work retains the attempt reservation. No process
        // cleanup or safe release can be inferred from an interrupted caller.
        let changed=sqlx::query("UPDATE local_attempts SET state=?,ended_at_us=? WHERE workspace_id=? AND id=? AND state IN ('claimed','running','draining') AND epoch=?")
            .bind(if unresolved.is_empty(){"released"}else{"draining"}).bind(if unresolved.is_empty(){Some(a.now.unix_micros())}else{None})
            .bind(a.workspace.as_str()).bind(seed.lease.attempt_id.as_str()).bind(seed.lease.epoch.value() as i64).execute(&mut *tx).await?;
        if changed.rows_affected() != 1 {
            return Err(TurnError::StaleAuthority);
        }
        save_progress(
            &mut tx,
            &a,
            &seed,
            snap.version,
            event,
            "execution.outcome",
            &bytes,
        )
        .await?;
        #[cfg(test)]
        tests::barrier(&self.status.database_path, "outcome_before_commit")?;
        tx.commit().await?;
        #[cfg(test)]
        tests::barrier(&self.status.database_path, "outcome_after_commit")?;
        Ok(())
    }
}
