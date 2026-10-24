//! Durable prepared-turn metadata. Publication never grants model/tool dispatch.
use super::*;
use avencrew_contracts::{
    scalars::Digest,
    wire::{ExecutionAuthority, Placement, RpcRequestHarnessStartTurnBody},
};
use std::collections::BTreeSet;
const MEDIA: &str = "application/vnd.avencrew.local-turn+json";
const LEASE_MEDIA: &str = "application/vnd.avencrew.lease+json";

pub enum TurnError {
    Store(StoreError),
    InvalidSeed,
    Unsupported,
    NotFoundOrDenied,
    Conflict,
    IdempotencyMismatch,
    StaleAuthority,
    Cancelled,
}
impl fmt::Display for TurnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Store(_) => "turn storage failure",
            Self::InvalidSeed => "invalid turn seed",
            Self::Unsupported => "turn profile unsupported",
            Self::NotFoundOrDenied => "turn unavailable",
            Self::Conflict => "turn version conflict",
            Self::IdempotencyMismatch => "turn identity mismatch",
            Self::StaleAuthority => "turn authority stale",
            Self::Cancelled => "turn cancellation retained",
        })
    }
}
impl fmt::Debug for TurnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
impl std::error::Error for TurnError {}
impl From<StoreError> for TurnError {
    fn from(e: StoreError) -> Self {
        Self::Store(e)
    }
}
impl From<sqlx::Error> for TurnError {
    fn from(e: sqlx::Error) -> Self {
        Self::Store(e.into())
    }
}
impl From<ControlError> for TurnError {
    fn from(e: ControlError) -> Self {
        match e {
            ControlError::Store(e) => Self::Store(e),
            ControlError::Conflict => Self::Conflict,
            ControlError::IdempotencyMismatch => Self::IdempotencyMismatch,
            ControlError::StaleAuthority => Self::StaleAuthority,
            ControlError::Cancelled => Self::Cancelled,
            ControlError::Unsupported => Self::Unsupported,
            ControlError::NotFoundOrDenied => Self::NotFoundOrDenied,
            _ => Self::InvalidSeed,
        }
    }
}
/// Protected coordinator input. Clock observations are not identity/digest fields.
pub struct LocalTurnPublication<'a> {
    pub actor: &'a DomainId,
    pub seed: &'a RpcRequestHarnessStartTurnBody,
    pub ordinal: &'a PositiveCounter,
    pub expected_run_version: &'a PositiveCounter,
    pub spec_blob: &'a DomainId,
    pub lease_blob: &'a DomainId,
    pub event: &'a DomainId,
    pub harness_build_digest: &'a Digest,
    pub at: &'a Instant,
    pub now: &'a Instant,
    pub current_boot: &'a DomainId,
    pub monotonic_now: &'a Counter,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalTurnReceipt {
    pub turn_id: DomainId,
    pub attempt_id: DomainId,
    pub run_id: DomainId,
    pub ordinal: PositiveCounter,
    pub event_id: DomainId,
    pub event_sequence: PositiveCounter,
    pub run_version: PositiveCounter,
    pub spec_blob_id: DomainId,
    pub lease_blob_id: DomainId,
}
/// Sensitive retained input has no Debug implementation. Receipt is metadata,
/// not a valid execution lease or a policy/budget admission acknowledgement.
pub struct RetainedLocalTurn {
    pub seed: RpcRequestHarnessStartTurnBody,
    pub receipt: LocalTurnReceipt,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Data {
    schema_version: u32,
    actor_id: DomainId,
    seed: RpcRequestHarnessStartTurnBody,
    expected_run_version: PositiveCounter,
    harness_build_digest: Digest,
    created_at: Instant,
    receipt: LocalTurnReceipt,
}
impl LocalStore {
    pub async fn publish_local_turn(
        &mut self,
        p: LocalTurnPublication<'_>,
    ) -> Result<LocalTurnReceipt, TurnError> {
        p.seed.validate().map_err(|_| TurnError::InvalidSeed)?;
        let spec = &p.seed.run_spec;
        let lease = &p.seed.lease;
        let w = &spec.workspace_id;
        if lease.run_id != spec.run_id
            || lease.authority != ExecutionAuthority::Local
            || lease.placement != Placement::Local
            || spec.placement != Placement::Local
        {
            return Err(TurnError::StaleAuthority);
        }
        // These owners do not exist yet. Metadata cannot imply their admission.
        if spec.worker_id.0.is_some()
            || spec.responsibility_id.0.is_some()
            || spec.project_id.0.is_some()
            || !spec.budget_period_refs.as_slice().is_empty()
            || spec.environment_recipe_version_id.0.is_some()
            || lease.environment_id.0.is_some()
            || p.seed.checkpoint_id.0.is_some()
        {
            return Err(TurnError::Unsupported);
        }
        let ids = [
            &p.seed.turn_id,
            &lease.attempt_id,
            &spec.session_id,
            &spec.branch_id,
            p.spec_blob,
            p.lease_blob,
            p.event,
        ];
        if ids.iter().collect::<BTreeSet<_>>().len() != ids.len()
            || ids
                .iter()
                .any(|id| **id == spec.run_id || **id == spec.task_id)
        {
            return Err(TurnError::InvalidSeed);
        }
        let identity = self.verified_identity(w, p.actor).await?;
        if spec.acting_principal_id != identity.principal
            || spec.owner_id != identity.principal
            || spec.requester_id != identity.principal
            || spec.audience_policy_id != identity.audience
            || spec.execution_policy_id != identity.execution
        {
            return Err(TurnError::NotFoundOrDenied);
        }
        let task = self.read_task_bundle(w, p.actor, &spec.task_id).await?;
        if task.receipt.version_id != spec.task_revision_id {
            return Err(TurnError::Conflict);
        }
        let bundle: serde_json::Value =
            serde_json::from_slice(&task.canonical_bytes).map_err(|_| TurnError::InvalidSeed)?;
        let revision = bundle["records"]
            .as_array()
            .and_then(|records| {
                records.iter().find(|r| {
                    r["table"] == "task_revisions" && r["id"] == spec.task_revision_id.as_str()
                })
            })
            .ok_or(TurnError::InvalidSeed)?;
        if revision["record"]["objective_version_id"] != spec.objective_version_id.as_str() {
            return Err(TurnError::Conflict);
        }
        if let Some(row) =
            sqlx::query("SELECT spec_blob_id FROM local_turns WHERE workspace_id=? AND id=?")
                .bind(w.as_str())
                .bind(p.seed.turn_id.as_str())
                .fetch_optional(&mut self.connection)
                .await?
        {
            let old = self.load_turn_data(w, p.actor, &p.seed.turn_id).await?;
            if old.seed != *p.seed
                || old.actor_id != *p.actor
                || old.expected_run_version != *p.expected_run_version
                || old.created_at != *p.at
                || old.harness_build_digest != *p.harness_build_digest
                || old.receipt.ordinal != *p.ordinal
                || old.receipt.event_id != *p.event
                || old.receipt.lease_blob_id != *p.lease_blob
                || row.try_get::<String, _>("spec_blob_id")? != p.spec_blob.as_str()
            {
                return Err(TurnError::IdempotencyMismatch);
            }
            return Ok(old.receipt); // acknowledged historical metadata; never reclaims a lease
        }
        if lease.boot_id != *p.current_boot
            || lease.expires_at <= *p.now
            || lease.monotonic_deadline_ns.value() <= p.monotonic_now.value()
            || *p.at > *p.now
            || lease.expires_at <= *p.at
        {
            return Err(TurnError::StaleAuthority);
        }
        if identity
            .blob_ids()
            .any(|id| id == p.spec_blob || id == p.lease_blob)
        {
            return Err(TurnError::Conflict);
        }
        let row = run_row(&mut self.connection, w, &spec.run_id, p.actor).await?;
        require_local(&row, identity.device())?;
        if row.cancelled {
            return Err(TurnError::Cancelled);
        }
        if row.status != "queued"
            || row.version != p.expected_run_version.value() as i64
            || row.revision != spec.task_revision_id
            || row.task != spec.task_id
            || row.accepted != p.seed.accepted_input_cut.value() as i64
            || spec.accepted_input_seq.value() > p.seed.accepted_input_cut.value()
        {
            return Err(TurnError::Conflict);
        }
        if lease.epoch.value() as i64 != next(row.epoch)? || p.ordinal.value() != 1 {
            return Err(TurnError::StaleAuthority);
        }
        let receipt = LocalTurnReceipt {
            turn_id: p.seed.turn_id.clone(),
            attempt_id: lease.attempt_id.clone(),
            run_id: spec.run_id.clone(),
            ordinal: p.ordinal.clone(),
            event_id: p.event.clone(),
            event_sequence: positive(next(row.event)?)?,
            run_version: positive(next(row.version)?)?,
            spec_blob_id: p.spec_blob.clone(),
            lease_blob_id: p.lease_blob.clone(),
        };
        let data = Data {
            schema_version: 1,
            actor_id: p.actor.clone(),
            seed: p.seed.clone(),
            expected_run_version: p.expected_run_version.clone(),
            harness_build_digest: p.harness_build_digest.clone(),
            created_at: p.at.clone(),
            receipt: receipt.clone(),
        };
        let bytes = canonical(&data)?;
        let lease_bytes = canonical(lease)?;
        self.put_canonical(w, p.spec_blob, &bytes)?;
        self.put_canonical(w, p.lease_blob, &lease_bytes)?;
        let mut tx = self.connection.begin().await?;
        let live = run_row(&mut tx, w, &spec.run_id, p.actor).await?;
        require_local(&live, identity.device())?;
        identity.check_live(&mut tx).await?;
        if live.cancelled {
            return Err(TurnError::Cancelled);
        }
        if live.version != row.version
            || live.epoch != row.epoch
            || live.status != "queued"
            || live.task != row.task
            || live.revision != row.revision
            || live.accepted != row.accepted
            || live.applied != row.applied
            || live.event != row.event
        {
            return Err(TurnError::Conflict);
        }
        blob(
            &mut tx,
            w,
            p.actor,
            p.spec_blob,
            &bytes,
            MEDIA,
            p.at.unix_micros(),
        )
        .await?;
        blob(
            &mut tx,
            w,
            p.actor,
            p.lease_blob,
            &lease_bytes,
            LEASE_MEDIA,
            p.at.unix_micros(),
        )
        .await?;
        // This first profile creates a private root session/branch atomically;
        // no caller-selected existing conversation is attached or overwritten.
        sqlx::query("INSERT INTO local_sessions (id,workspace_id,created_at_us,owner_actor_id,title,status) VALUES (?,?,?,?,'Local turn','open')").bind(spec.session_id.as_str()).bind(w.as_str()).bind(p.at.unix_micros()).bind(p.actor.as_str()).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO local_session_branches (id,workspace_id,created_at_us,session_id,parent_branch_id,fork_item_id,last_seq) VALUES (?,?,?,?,NULL,NULL,0)").bind(spec.branch_id.as_str()).bind(w.as_str()).bind(p.at.unix_micros()).bind(spec.session_id.as_str()).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO local_attempts (id,workspace_id,created_at_us,run_id,epoch,placement,state,harness_build_digest,protocol_version,lease_receipt_blob_id,lease_expires_at_us,monotonic_deadline_ns,boot_id,started_at_us,ended_at_us) VALUES (?,?,?,?,?,'local','claimed',?,1,?,?,?,?,?,NULL)").bind(lease.attempt_id.as_str()).bind(w.as_str()).bind(p.at.unix_micros()).bind(spec.run_id.as_str()).bind(lease.epoch.value() as i64).bind(p.harness_build_digest.as_str()).bind(p.lease_blob.as_str()).bind(lease.expires_at.unix_micros()).bind(lease.monotonic_deadline_ns.value() as i64).bind(lease.boot_id.as_str()).bind(p.at.unix_micros()).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO local_turns (id,workspace_id,created_at_us,run_id,attempt_id,ordinal,state,input_command_seq,started_at_us,ended_at_us,spec_blob_id) VALUES (?,?,?,?,?,?,'prepared',?,?,NULL,?)").bind(p.seed.turn_id.as_str()).bind(w.as_str()).bind(p.at.unix_micros()).bind(spec.run_id.as_str()).bind(lease.attempt_id.as_str()).bind(p.ordinal.value() as i64).bind(p.seed.accepted_input_cut.value() as i64).bind(p.at.unix_micros()).bind(p.spec_blob.as_str()).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO local_events (id,workspace_id,created_at_us,run_id,seq,attempt_id,epoch,event_type,schema_version,occurred_at_us,payload_blob_id,command_id) VALUES (?,?,?,?,?,?,?,'turn.prepared',1,?,?,NULL)").bind(p.event.as_str()).bind(w.as_str()).bind(p.at.unix_micros()).bind(spec.run_id.as_str()).bind(receipt.event_sequence.value() as i64).bind(lease.attempt_id.as_str()).bind(lease.epoch.value() as i64).bind(p.at.unix_micros()).bind(p.spec_blob.as_str()).execute(&mut *tx).await?;
        sqlx::query("UPDATE local_runs SET status='preparing',fencing_epoch=?,row_version=?,last_event_seq=?,session_id=?,branch_id=? WHERE workspace_id=? AND id=?").bind(lease.epoch.value() as i64).bind(receipt.run_version.value() as i64).bind(receipt.event_sequence.value() as i64).bind(spec.session_id.as_str()).bind(spec.branch_id.as_str()).bind(w.as_str()).bind(spec.run_id.as_str()).execute(&mut *tx).await?;
        #[cfg(test)]
        tests::barrier(&self.status.database_path, "before_commit")?;
        tx.commit().await?;
        #[cfg(test)]
        tests::barrier(&self.status.database_path, "after_commit")?;
        Ok(receipt)
    }
    pub async fn read_local_turn(
        &mut self,
        workspace: &DomainId,
        actor: &DomainId,
        turn: &DomainId,
    ) -> Result<RetainedLocalTurn, TurnError> {
        let data = self.load_turn_data(workspace, actor, turn).await?;
        Ok(RetainedLocalTurn {
            seed: data.seed,
            receipt: data.receipt,
        })
    }
    async fn load_turn_data(
        &mut self,
        w: &DomainId,
        a: &DomainId,
        turn: &DomainId,
    ) -> Result<Data, TurnError> {
        let identity = self.verified_identity(w, a).await?;
        let row = sqlx::query("SELECT * FROM local_turns WHERE workspace_id=? AND id=?")
            .bind(w.as_str())
            .bind(turn.as_str())
            .fetch_optional(&mut self.connection)
            .await?
            .ok_or(TurnError::NotFoundOrDenied)?;
        let run = DomainId::new(row.try_get::<String, _>("run_id")?)
            .map_err(|_| TurnError::InvalidSeed)?;
        let parent = run_row(&mut self.connection, w, &run, a).await?;
        self.read_task_bundle(w, a, &parent.task).await?;
        let id = DomainId::new(row.try_get::<String, _>("spec_blob_id")?)
            .map_err(|_| TurnError::InvalidSeed)?;
        let bytes = self.read_registered_blob(w, a, &id, MEDIA).await?;
        let d: Data = serde_json::from_slice(&bytes).map_err(|_| TurnError::InvalidSeed)?;
        d.seed.validate().map_err(|_| TurnError::InvalidSeed)?;
        if d.seed.run_spec.acting_principal_id != identity.principal
            || d.seed.run_spec.owner_id != identity.principal
            || d.seed.run_spec.audience_policy_id != identity.audience
            || d.seed.run_spec.execution_policy_id != identity.execution
            || d.seed.run_spec.task_id != parent.task
            || d.seed.run_spec.task_revision_id != parent.revision
            || d.seed.lease.run_id != run
            || d.seed.lease.attempt_id != d.receipt.attempt_id
            || d.receipt.run_version.value()
                != d.expected_run_version
                    .value()
                    .checked_add(1)
                    .ok_or(TurnError::InvalidSeed)?
            || parent.event < d.receipt.event_sequence.value() as i64
            || row.try_get::<i64, _>("started_at_us")? != d.created_at.unix_micros()
            || canonical(&d)? != bytes
            || d.schema_version != 1
            || d.actor_id != *a
            || d.seed.run_spec.workspace_id != *w
            || d.seed.turn_id != *turn
            || d.seed.run_spec.run_id != run
            || d.receipt.turn_id != *turn
            || d.receipt.run_id != run
            || d.receipt.spec_blob_id != id
            || d.receipt.attempt_id.as_str() != row.try_get::<String, _>("attempt_id")?
            || d.receipt.ordinal.value() as i64 != row.try_get::<i64, _>("ordinal")?
            || d.seed.accepted_input_cut.value() as i64
                != row.try_get::<i64, _>("input_command_seq")?
            || d.created_at.unix_micros() != row.try_get::<i64, _>("created_at_us")?
            || row.try_get::<String, _>("state")? != "prepared"
        {
            return Err(TurnError::IdempotencyMismatch);
        }
        let lease = self
            .read_registered_blob(w, a, &d.receipt.lease_blob_id, LEASE_MEDIA)
            .await?;
        if lease != canonical(&d.seed.lease)? {
            return Err(TurnError::IdempotencyMismatch);
        }
        let attempt =
            sqlx::query("SELECT * FROM local_attempts WHERE workspace_id=? AND id=? AND run_id=?")
                .bind(w.as_str())
                .bind(d.receipt.attempt_id.as_str())
                .bind(run.as_str())
                .fetch_one(&mut self.connection)
                .await?;
        if attempt.try_get::<String, _>("placement")? != "local"
            || attempt.try_get::<i64, _>("protocol_version")? != 1
            || attempt.try_get::<i64, _>("created_at_us")? != d.created_at.unix_micros()
            || attempt.try_get::<i64, _>("started_at_us")? != d.created_at.unix_micros()
            || attempt.try_get::<i64, _>("epoch")? != d.seed.lease.epoch.value() as i64
            || attempt.try_get::<String, _>("lease_receipt_blob_id")?
                != d.receipt.lease_blob_id.as_str()
            || attempt.try_get::<String, _>("harness_build_digest")?
                != d.harness_build_digest.as_str()
            || attempt.try_get::<String, _>("boot_id")? != d.seed.lease.boot_id.as_str()
            || attempt.try_get::<i64, _>("lease_expires_at_us")?
                != d.seed.lease.expires_at.unix_micros()
            || attempt.try_get::<i64, _>("monotonic_deadline_ns")?
                != d.seed.lease.monotonic_deadline_ns.value() as i64
        {
            return Err(TurnError::IdempotencyMismatch);
        }
        let event =
            sqlx::query("SELECT * FROM local_events WHERE workspace_id=? AND id=? AND run_id=?")
                .bind(w.as_str())
                .bind(d.receipt.event_id.as_str())
                .bind(run.as_str())
                .fetch_one(&mut self.connection)
                .await?;
        if event.try_get::<i64, _>("schema_version")? != 1
            || event.try_get::<i64, _>("created_at_us")? != d.created_at.unix_micros()
            || event.try_get::<i64, _>("occurred_at_us")? != d.created_at.unix_micros()
            || event.try_get::<Option<String>, _>("command_id")?.is_some()
            || event.try_get::<i64, _>("seq")? != d.receipt.event_sequence.value() as i64
            || event.try_get::<String, _>("event_type")? != "turn.prepared"
            || event.try_get::<String, _>("payload_blob_id")? != id.as_str()
            || event.try_get::<String, _>("attempt_id")? != d.seed.lease.attempt_id.as_str()
            || event.try_get::<i64, _>("epoch")? != d.seed.lease.epoch.value() as i64
        {
            return Err(TurnError::IdempotencyMismatch);
        }
        let conversation: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM local_sessions s JOIN local_session_branches b ON b.workspace_id=s.workspace_id AND b.session_id=s.id WHERE s.workspace_id=? AND s.id=? AND s.owner_actor_id=? AND b.id=?)").bind(w.as_str()).bind(d.seed.run_spec.session_id.as_str()).bind(a.as_str()).bind(d.seed.run_spec.branch_id.as_str()).fetch_one(&mut self.connection).await?;
        if !conversation {
            return Err(TurnError::IdempotencyMismatch);
        }
        identity.check_live(&mut self.connection).await?;
        Ok(d)
    }
}
#[cfg(test)]
pub(super) mod tests;
