//! One durable proposal/dispatch/result path for the fixture read profile.
use super::*;
use avencrew_contracts::{
    runtime::{Effect, LocalProposal, Origin},
    wire::{ProtocolVersion, ToolResult, WireMessage},
};
const PROPOSAL_MEDIA: &str = "application/vnd.avencrew.invocation-proposal+json";
const RESULT_MEDIA: &str = "application/vnd.avencrew.invocation-result+json";
const POLICY_MEDIA: &str = "application/vnd.avencrew.dispatch-policy+json";

pub enum LocalDispatch {
    Fresh,
    Result(Box<ToolResult>),
    Unknown,
}

impl LocalStore {
    /// Persist the observation lost when a broker future is dropped. Never
    /// replace a terminal result or invent a physical dispatch for a proposal.
    pub async fn record_local_unknown_invocation(
        &mut self,
        a: LocalExecutionAccess<'_>,
        step: &DomainId,
        invocation: &DomainId,
        code: avencrew_contracts::wire::ErrorCode,
        event: &DomainId,
    ) -> Result<(), TurnError> {
        let _proof_scope = self.proof_read_scope();
        use avencrew_contracts::wire::ErrorCode;
        if !matches!(code, ErrorCode::Timeout | ErrorCode::UnknownOutcome) {
            return Err(TurnError::InvalidSeed);
        }
        let seed = self.authorize_local_execution(&a, true).await?;
        self.read_local_step(a.workspace, a.actor, a.turn, step)
            .await?;
        let p = self
            .read_local_proposal(a.workspace, a.actor, invocation)
            .await?;
        if p.context.step_id != *step || p.context.run_id != seed.run_spec.run_id {
            return Err(TurnError::Conflict);
        }
        let row = sqlx::query(
            "SELECT state,result_blob_id FROM local_invocations WHERE workspace_id=? AND id=?",
        )
        .bind(a.workspace.as_str())
        .bind(invocation.as_str())
        .fetch_one(&mut self.connection)
        .await?;
        if row
            .try_get::<Option<String>, _>("result_blob_id")?
            .is_some()
        {
            self.read_local_tool_result(a.workspace, a.actor, invocation)
                .await?;
            return Ok(());
        }
        if row.try_get::<String, _>("state")? == "proposed" {
            return Ok(());
        }
        let result:ToolResult=serde_json::from_value(serde_json::json!({"status":"unknown","invocation_id":invocation,"schema_version":"1.0","artifact_refs":[],"receipt_ref":null,"truncated":false,"next_cursor":null,"usage_refs":[],"error":{"code":code,"message":"tool completion was not acknowledged","evidence_refs":[]},"data":null})).map_err(|_|TurnError::InvalidSeed)?;
        self.record_local_tool_result(a, &p, Some(&result), event)
            .await
    }
    pub async fn commit_local_proposals(
        &mut self,
        a: LocalExecutionAccess<'_>,
        proposals: &[LocalProposal],
        event: &DomainId,
    ) -> Result<(), TurnError> {
        let _proof_scope = self.proof_read_scope();
        let seed = self.authorize_local_execution(&a, false).await?;
        let snap = self
            .local_execution_snapshot(a.workspace, a.actor, a.turn)
            .await?;
        let step = snap.step.as_ref().ok_or(TurnError::Conflict)?;
        let completion = snap
            .model
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .ok_or(TurnError::Conflict)?;
        if step.accepted_input_cut != snap.progress.state.applied_input_cut
            || completion.response.finish != NormalizedModelResponseFinish::Tools
            || proposals.len() != completion.proposal_identities.len()
            || proposals.len() > 100
        {
            return Err(TurnError::InvalidSeed);
        }
        let old = sqlx::query(
            "SELECT id FROM local_invocations WHERE workspace_id=? AND step_id=? ORDER BY id",
        )
        .bind(a.workspace.as_str())
        .bind(step.binding.step_id.as_str())
        .fetch_all(&mut self.connection)
        .await?;
        if !old.is_empty() {
            if old.len() != proposals.len() {
                return Err(TurnError::IdempotencyMismatch);
            }
            for p in proposals {
                if self
                    .read_local_proposal(a.workspace, a.actor, &p.invocation_id)
                    .await?
                    != *p
                {
                    return Err(TurnError::IdempotencyMismatch);
                }
            }
            return Ok(());
        }
        let total: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM local_invocations WHERE workspace_id=? AND run_id=?",
        )
        .bind(a.workspace.as_str())
        .bind(seed.run_spec.run_id.as_str())
        .fetch_one(&mut self.connection)
        .await?;
        if total + proposals.len() as i64 > budget::TOOL_CALLS {
            return Err(TurnError::InvalidSeed);
        }
        let mut blobs = Vec::new();
        let mut ids = std::collections::BTreeSet::new();
        for (p, identity) in proposals.iter().zip(&completion.proposal_identities) {
            if !ids.insert(&p.invocation_id) || p.invocation_id != identity.invocation_id {
                return Err(TurnError::InvalidSeed);
            }
            validate_proposal(step, completion, p)?;
            p.validate().map_err(|_| TurnError::InvalidSeed)?;
            let bytes =
                avencrew_contracts::runtime::record_bytes(p).map_err(|_| TurnError::InvalidSeed)?;
            self.put_canonical(a.workspace, &p.invocation_id, &bytes)?;
            blobs.push((p, bytes));
        }
        let progress_bytes = progress_bytes(a.turn, snap.version + 1, &snap.progress)?;
        self.put_canonical(a.workspace, event, &progress_bytes)?;
        let identity = self.verified_identity(a.workspace, a.actor).await?;
        let mut tx = self.connection.begin().await?;
        identity.check_live(&mut tx).await?;
        check_dispatch_cut(&mut tx, &a, &seed).await?;
        for (p, bytes) in &blobs {
            blob(
                &mut tx,
                a.workspace,
                a.actor,
                &p.invocation_id,
                bytes,
                PROPOSAL_MEDIA,
                a.now.unix_micros(),
            )
            .await?;
            let Origin::Model {
                provider_call_id, ..
            } = &p.origin
            else {
                return Err(TurnError::Unsupported);
            };
            sqlx::query("INSERT INTO local_invocations (id,workspace_id,created_at_us,run_id,step_id,parent_invocation_id,provider_call_id,request_origin,tool_binding_key,idempotency_key,arguments_blob_id,arguments_digest,state) VALUES (?,?,?,?,?,NULL,?,'model',?,?,?,?,'proposed')")
                .bind(p.invocation_id.as_str()).bind(a.workspace.as_str()).bind(a.now.unix_micros()).bind(seed.run_spec.run_id.as_str()).bind(step.binding.step_id.as_str()).bind(provider_call_id).bind(p.context.binding_id.as_str()).bind(p.invocation_id.as_str()).bind(p.invocation_id.as_str()).bind(digest_bytes(p.arguments_digest.as_str())).execute(&mut *tx).await?;
        }
        save_progress(
            &mut tx,
            &a,
            &seed,
            snap.version,
            event,
            "execution.proposals",
            &progress_bytes,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn read_local_proposal(
        &mut self,
        w: &DomainId,
        actor: &DomainId,
        id: &DomainId,
    ) -> Result<LocalProposal, TurnError> {
        let _proof_scope = self.proof_read_scope();
        self.verified_identity(w, actor).await?;
        let row = sqlx::query("SELECT * FROM local_invocations WHERE workspace_id=? AND id=?")
            .bind(w.as_str())
            .bind(id.as_str())
            .fetch_one(&mut self.connection)
            .await?;
        let blob_id = DomainId::new(row.try_get::<String, _>("arguments_blob_id")?)
            .map_err(|_| TurnError::Conflict)?;
        let bytes = self
            .read_registered_blob(w, actor, &blob_id, PROPOSAL_MEDIA)
            .await?;
        let p: LocalProposal = serde_json::from_slice(&bytes).map_err(|_| TurnError::Conflict)?;
        if !matches!(&p.origin,Origin::Model{provider_call_id,..} if Some(provider_call_id.as_str())==row.try_get::<Option<String>,_>("provider_call_id")?.as_deref())
            || row.try_get::<String, _>("request_origin")? != "model"
            || row.try_get::<String, _>("idempotency_key")? != id.as_str()
            || avencrew_contracts::runtime::record_bytes(&p).map_err(|_| TurnError::InvalidSeed)?
                != bytes
            || p.invocation_id != *id
            || p.context.workspace_id != *w
            || p.context.run_id.as_str() != row.try_get::<String, _>("run_id")?
            || p.context.step_id.as_str() != row.try_get::<String, _>("step_id")?
            || p.context.binding_id.as_str() != row.try_get::<String, _>("tool_binding_key")?
            || digest_bytes(p.arguments_digest.as_str())
                != row.try_get::<Vec<u8>, _>("arguments_digest")?
        {
            return Err(TurnError::Conflict);
        }
        p.validate().map_err(|_| TurnError::InvalidSeed)?;
        Ok(p)
    }

    /// The only executable profile is a bounded read of explicitly supplied synthetic sources.
    /// No mutation, filesystem path, PTC parent or external provider can be admitted.
    pub async fn claim_local_dispatch(
        &mut self,
        a: LocalExecutionAccess<'_>,
        proposal: &LocalProposal,
        policy_digest: &avencrew_contracts::scalars::Digest,
        event: &DomainId,
    ) -> Result<LocalDispatch, TurnError> {
        let _proof_scope = self.proof_read_scope();
        let seed = self.authorize_local_execution(&a, false).await?;
        if self
            .read_local_proposal(a.workspace, a.actor, &proposal.invocation_id)
            .await?
            != *proposal
        {
            return Err(TurnError::IdempotencyMismatch);
        }
        let snap = self
            .local_execution_snapshot(a.workspace, a.actor, a.turn)
            .await?;
        let step = snap.step.as_ref().ok_or(TurnError::Conflict)?;
        let binding = step
            .bindings
            .iter()
            .find(|b| b.binding_id == proposal.context.binding_id)
            .ok_or(TurnError::InvalidSeed)?;
        if step.accepted_input_cut != snap.progress.state.applied_input_cut
            || binding.name != "fixture.read"
            || binding.effect != Effect::Read
            || proposal.effect != Effect::Read
            || !matches!(proposal.origin, Origin::Model { .. })
            || step.binding.step_id != proposal.context.step_id
        {
            return Err(TurnError::Unsupported);
        }
        let row = sqlx::query(
            "SELECT state,result_blob_id FROM local_invocations WHERE workspace_id=? AND id=?",
        )
        .bind(a.workspace.as_str())
        .bind(proposal.invocation_id.as_str())
        .fetch_one(&mut self.connection)
        .await?;
        match row.try_get::<String, _>("state")?.as_str() {
            "result" => {
                return Ok(LocalDispatch::Result(Box::new(
                    self.read_local_tool_result(a.workspace, a.actor, &proposal.invocation_id)
                        .await?,
                )))
            }
            "dispatched" | "unknown" => return Ok(LocalDispatch::Unknown),
            "proposed" => {}
            _ => return Err(TurnError::Conflict),
        }
        if snap.unresolved_invocations.len() + snap.unresolved_model_calls.len() >= 100 {
            return Err(TurnError::Conflict);
        }
        let bytes = canonical(policy_digest)?;
        self.put_canonical(a.workspace, event, &bytes)?;
        // A separate event locator carries progress so policy bytes cannot be confused with state.
        let identity = self.verified_identity(a.workspace, a.actor).await?;
        let mut tx = self.connection.begin().await?;
        identity.check_live(&mut tx).await?;
        check_dispatch_cut(&mut tx, &a, &seed).await?;
        blob(
            &mut tx,
            a.workspace,
            a.actor,
            event,
            &bytes,
            POLICY_MEDIA,
            a.now.unix_micros(),
        )
        .await?;
        let changed=sqlx::query("UPDATE local_invocations SET state='dispatched',policy_receipt_blob_id=? WHERE workspace_id=? AND id=? AND state='proposed'").bind(event.as_str()).bind(a.workspace.as_str()).bind(proposal.invocation_id.as_str()).execute(&mut *tx).await?;
        if changed.rows_affected() != 1 {
            return Err(TurnError::Conflict);
        }
        sqlx::query("INSERT INTO local_dispatches (id,workspace_id,created_at_us,invocation_id,attempt_id,ordinal,epoch,state,started_at_us) VALUES (?,?,?,?,?,1,?,'started',?)")
            .bind(event.as_str()).bind(a.workspace.as_str()).bind(a.now.unix_micros()).bind(proposal.invocation_id.as_str()).bind(seed.lease.attempt_id.as_str()).bind(seed.lease.epoch.value() as i64).bind(a.now.unix_micros()).execute(&mut *tx).await?;
        budget::record_usage(
            &mut tx,
            &a,
            &seed,
            budget::Measurement {
                id: event,
                invocation: Some(&proposal.invocation_id),
                meter: "tool_dispatch",
                source: &proposal.invocation_id,
                unit: "call",
                quantity: Some(1),
                price: &step.model_call_id,
            },
        )
        .await?;
        append_execution_event(&mut tx, &a, &seed, event, "execution.dispatch", event).await?;
        #[cfg(test)]
        tests::barrier(&self.status.database_path, "dispatch_before_commit")?;
        tx.commit().await?;
        #[cfg(test)]
        tests::barrier(&self.status.database_path, "dispatch_after_commit")?;
        Ok(LocalDispatch::Fresh)
    }

    pub async fn read_local_tool_result(
        &mut self,
        w: &DomainId,
        actor: &DomainId,
        id: &DomainId,
    ) -> Result<ToolResult, TurnError> {
        let _proof_scope = self.proof_read_scope();
        self.read_local_proposal(w, actor, id).await?;
        let pointer:String=sqlx::query_scalar("SELECT result_blob_id FROM local_invocations WHERE workspace_id=? AND id=? AND state IN ('result','unknown') AND result_blob_id IS NOT NULL").bind(w.as_str()).bind(id.as_str()).fetch_one(&mut self.connection).await?;
        let pointer = DomainId::new(pointer).map_err(|_| TurnError::Conflict)?;
        let bytes = self
            .read_registered_blob(w, actor, &pointer, RESULT_MEDIA)
            .await?;
        let result: ToolResult = serde_json::from_slice(&bytes).map_err(|_| TurnError::Conflict)?;
        if avencrew_contracts::runtime::record_bytes(&result).map_err(|_| TurnError::InvalidSeed)?
            != bytes
        {
            return Err(TurnError::Conflict);
        }
        result.validate().map_err(|_| TurnError::InvalidSeed)?;
        if serde_json::to_value(&result).map_err(|_| TurnError::Conflict)?["invocation_id"]
            != id.as_str()
        {
            return Err(TurnError::Conflict);
        }
        Ok(result)
    }

    pub async fn record_local_tool_result(
        &mut self,
        a: LocalExecutionAccess<'_>,
        proposal: &LocalProposal,
        result: Option<&ToolResult>,
        event: &DomainId,
    ) -> Result<(), TurnError> {
        let _proof_scope = self.proof_read_scope();
        let seed = self.authorize_local_execution(&a, true).await?;
        if self
            .read_local_proposal(a.workspace, a.actor, &proposal.invocation_id)
            .await?
            != *proposal
        {
            return Err(TurnError::IdempotencyMismatch);
        }
        let state: String =
            sqlx::query_scalar("SELECT state FROM local_invocations WHERE workspace_id=? AND id=?")
                .bind(a.workspace.as_str())
                .bind(proposal.invocation_id.as_str())
                .fetch_one(&mut self.connection)
                .await?;
        let has_result:bool=sqlx::query_scalar("SELECT result_blob_id IS NOT NULL FROM local_invocations WHERE workspace_id=? AND id=?").bind(a.workspace.as_str()).bind(proposal.invocation_id.as_str()).fetch_one(&mut self.connection).await?;
        if has_result {
            let old = self
                .read_local_tool_result(a.workspace, a.actor, &proposal.invocation_id)
                .await?;
            return if result == Some(&old) {
                Ok(())
            } else {
                Err(TurnError::IdempotencyMismatch)
            };
        }
        if state == "unknown" && result.is_none() {
            return Ok(());
        }
        if !matches!(state.as_str(), "dispatched" | "unknown") {
            return Err(TurnError::Conflict);
        }
        let bytes = if let Some(result) = result {
            avencrew_contracts::encode_message(&WireMessage::ToolResult {
                schema_version: ProtocolVersion::V10,
                payload: Box::new(result.clone()),
            })
            .map_err(|_| TurnError::InvalidSeed)?;
            let value = serde_json::to_value(result).map_err(|_| TurnError::InvalidSeed)?;
            if value["invocation_id"] != proposal.invocation_id.as_str()
                || matches!(
                    result,
                    ToolResult::Running { .. } | ToolResult::AwaitingInput { .. }
                )
            {
                return Err(TurnError::InvalidSeed);
            }
            let bytes = avencrew_contracts::runtime::record_bytes(result)
                .map_err(|_| TurnError::InvalidSeed)?;
            if bytes.len() > 262_144 {
                return Err(TurnError::InvalidSeed);
            }
            self.put_canonical(a.workspace, event, &bytes)?;
            Some(bytes)
        } else {
            None
        };
        let price:String=sqlx::query_scalar("SELECT s.model_profile_blob_id FROM local_steps s JOIN local_invocations i ON i.workspace_id=s.workspace_id AND i.step_id=s.id WHERE i.workspace_id=? AND i.id=?").bind(a.workspace.as_str()).bind(proposal.invocation_id.as_str()).fetch_one(&mut self.connection).await?;
        let price = DomainId::new(price).map_err(|_| TurnError::Conflict)?;
        let unknown = result
            .is_none_or(|r| matches!(r, ToolResult::Unknown { .. } | ToolResult::Lost { .. }));
        if bytes.is_none() {
            self.put_canonical(
                a.workspace,
                event,
                &canonical(
                    &serde_json::json!({"invocation_id":proposal.invocation_id,"state":"unknown"}),
                )?,
            )?;
        }
        let identity = self.verified_identity(a.workspace, a.actor).await?;
        let mut tx = self.connection.begin().await?;
        identity.check_live(&mut tx).await?;
        if let Some(bytes) = &bytes {
            blob(
                &mut tx,
                a.workspace,
                a.actor,
                event,
                bytes,
                RESULT_MEDIA,
                a.now.unix_micros(),
            )
            .await?;
        }
        sqlx::query("UPDATE local_invocations SET state=?,result_blob_id=? WHERE workspace_id=? AND id=? AND state IN ('dispatched','unknown')")
            .bind(if unknown{"unknown"}else{"result"}).bind(result.map(|_|event.as_str())).bind(a.workspace.as_str()).bind(proposal.invocation_id.as_str()).execute(&mut *tx).await?;
        sqlx::query("UPDATE local_dispatches SET state=?,ended_at_us=? WHERE workspace_id=? AND invocation_id=? AND state IN ('started','unknown')")
            .bind(if unknown{"unknown"}else{"result"}).bind(a.now.unix_micros()).bind(a.workspace.as_str()).bind(proposal.invocation_id.as_str()).execute(&mut *tx).await?;
        if bytes.is_none() {
            blob(
                &mut tx,
                a.workspace,
                a.actor,
                event,
                &canonical(
                    &serde_json::json!({"invocation_id":proposal.invocation_id,"state":"unknown"}),
                )?,
                "application/vnd.avencrew.execution-observation+json",
                a.now.unix_micros(),
            )
            .await?;
        }
        budget::record_usage(
            &mut tx,
            &a,
            &seed,
            budget::Measurement {
                id: event,
                invocation: Some(&proposal.invocation_id),
                meter: if bytes.is_some() {
                    "tool_result_bytes"
                } else {
                    "tool_result_unknown"
                },
                source: &proposal.invocation_id,
                unit: "byte",
                quantity: bytes.as_ref().map(|v| v.len() as u64),
                price: &price,
            },
        )
        .await?;
        append_execution_event(
            &mut tx,
            &a,
            &seed,
            event,
            if unknown {
                "execution.tool_unknown"
            } else {
                "execution.tool_result"
            },
            event,
        )
        .await?;
        #[cfg(test)]
        tests::barrier(&self.status.database_path, "result_before_commit")?;
        tx.commit().await?;
        #[cfg(test)]
        tests::barrier(&self.status.database_path, "result_after_commit")?;
        Ok(())
    }
}

pub(super) fn validate_proposal(
    step: &LocalStep,
    response: &avencrew_contracts::runtime::FixtureResult,
    p: &LocalProposal,
) -> Result<(), TurnError> {
    let identity = response
        .proposal_identities
        .iter()
        .find(|i| i.invocation_id == p.invocation_id)
        .ok_or(TurnError::Conflict)?;
    let Origin::Model {
        model_call_id,
        provider_call_id,
    } = &p.origin
    else {
        return Err(TurnError::Unsupported);
    };
    let item = response
        .response
        .items
        .as_slice()
        .get(identity.item as usize)
        .ok_or(TurnError::Conflict)?;
    let avencrew_contracts::wire::NormalizedModelResponseItemsItem::ToolProposal {
        binding_id,
        arguments,
        provider_call_id: provider,
    } = item
    else {
        return Err(TurnError::Conflict);
    };
    let mut context = step.context.clone();
    context.binding_id = binding_id.clone();
    let binding = step
        .bindings
        .iter()
        .find(|b| b.binding_id == *binding_id)
        .ok_or(TurnError::Conflict)?;
    if *model_call_id != step.model_call_id
        || *provider_call_id != identity.provider_call_id
        || provider.as_str() != provider_call_id
        || p.context != context
        || p.arguments != *arguments
        || p.effect != binding.effect
        || p.adapter_digest != binding.adapter_digest
    {
        return Err(TurnError::Conflict);
    }
    p.validate().map_err(|_| TurnError::Conflict)
}
