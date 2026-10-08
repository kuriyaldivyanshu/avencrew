//! Verified projection replay only. A recovery gate never grants dispatch.
use super::*;
use avencrew_contracts::wire::RunState;

const MAX_RUNS: i64 = 256;
const MAX_COMMANDS: i64 = 1024;
const MAX_EVENTS: i64 = 2048;
const MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryGate {
    Terminal,
    AuthorityHeld,
    CancellationPending,
    ReconciliationRequired,
    AwaitingControlConsumer,
}
/// Contains sensitive input; deliberately has no Debug or logging representation.
pub struct RecoveredControl {
    pub sequence: PositiveCounter,
    pub command: Command,
    /// Verified canonical journal bytes, independent of DTO serialization order.
    pub canonical_bytes: Vec<u8>,
    pub original_receipt: CommandReceipt,
}
pub struct RecoveredRun {
    pub run: DomainId,
    pub task: DomainId,
    pub task_revision: DomainId,
    pub state: RunState,
    pub version: PositiveCounter,
    pub epoch: Counter,
    pub accepted: Counter,
    pub applied: Counter,
    pub last_event: PositiveCounter,
    pub gate: RecoveryGate,
    pub pending_controls: Vec<RecoveredControl>,
}
/// Internal snapshot, not a portable wire message, checkpoint or permission grant.
pub struct RecoverySnapshot {
    pub runs: Vec<RecoveredRun>,
}
impl LocalStore {
    pub async fn recover_local_controls(
        &mut self,
        workspace: &DomainId,
        actor: &DomainId,
    ) -> Result<RecoverySnapshot, ControlError> {
        let identity = self.verified_identity(workspace, actor).await?;
        let ids:Vec<String>=sqlx::query_scalar("SELECT r.id FROM local_runs r JOIN local_tasks t ON t.workspace_id=r.workspace_id AND t.id=r.task_id WHERE r.workspace_id=? AND t.owner_actor_id=? ORDER BY r.id LIMIT ?")
            .bind(workspace.as_str()).bind(actor.as_str()).bind(MAX_RUNS+1).fetch_all(&mut self.connection).await?;
        if ids.len() > MAX_RUNS as usize {
            return Err(invalid("recovery run limit exceeded").into());
        }
        let mut runs = Vec::with_capacity(ids.len());
        let mut byte_count = 0usize;
        for id in ids {
            let run = DomainId::new(id).map_err(|_| invalid("invalid retained run ID"))?;
            let row = run_row(&mut self.connection, workspace, &run, actor).await?;
            let state: RunState =
                serde_json::from_value(serde_json::Value::String(row.status.clone()))
                    .map_err(|_| invalid("invalid retained run state"))?;
            let task = self.read_task_bundle(workspace, actor, &row.task).await?;
            if task.receipt.version_id != row.revision {
                return Err(ControlError::Conflict);
            }
            let events = sqlx::query(
                "SELECT * FROM local_events WHERE workspace_id=? AND run_id=? ORDER BY seq LIMIT ?",
            )
            .bind(workspace.as_str())
            .bind(run.as_str())
            .bind(MAX_EVENTS + 1)
            .fetch_all(&mut self.connection)
            .await?;
            if events.is_empty()
                || events.len() > MAX_EVENTS as usize
                || events.len() as i64 != row.event
            {
                return Err(invalid("recovery event high-water or limit mismatch").into());
            }
            let mut unsupported = false;
            for (n, event) in events.iter().enumerate() {
                if event.try_get::<i64, _>("seq")? != n as i64 + 1 {
                    return Err(invalid("recovery event sequence gap").into());
                }
                if n > 0 && event.try_get::<String, _>("event_type")? == "run.accepted" {
                    return Err(ControlError::Conflict);
                }
                unsupported |= !matches!(
                    event.try_get::<String, _>("event_type")?.as_str(),
                    "run.accepted" | "control.accepted"
                );
            }
            let first = &events[0];
            if first.try_get::<String, _>("event_type")? != "run.accepted" {
                return Err(ControlError::Conflict);
            }
            let payload = DomainId::new(first.try_get::<String, _>("payload_blob_id")?)
                .map_err(|_| invalid("missing registration payload"))?;
            let bytes = self
                .read_registered_blob(workspace, actor, &payload, RUN_MEDIA)
                .await?;
            let data: RegistrationData = serde_json::from_slice(&bytes)
                .map_err(|_| invalid("invalid recovery registration"))?;
            if canonical(&data)? != bytes
                || data.schema_version != 1
                || data.workspace_id != *workspace
                || data.actor_id != *actor
                || data.receipt.run_id != run
                || data.receipt.payload_blob_id != payload
                || data.receipt.event_id.as_str() != first.try_get::<String, _>("id")?
            {
                return Err(ControlError::Conflict);
            }
            let created: i64 = sqlx::query_scalar(
                "SELECT created_at_us FROM local_runs WHERE workspace_id=? AND id=?",
            )
            .bind(workspace.as_str())
            .bind(run.as_str())
            .fetch_one(&mut self.connection)
            .await?;
            if data.created_at.unix_micros() != created {
                return Err(ControlError::Conflict);
            }
            let receipt = &data.receipt;
            self.register_local_run(LocalRunRegistration {
                workspace,
                actor,
                task: &receipt.task_id,
                task_revision: &receipt.task_revision_id,
                run: &receipt.run_id,
                event: &receipt.event_id,
                queue: &receipt.queue_id,
                payload_blob: &receipt.payload_blob_id,
                at: &data.created_at,
            })
            .await?;
            let commands=sqlx::query("SELECT * FROM local_commands WHERE workspace_id=? AND run_id=? ORDER BY seq LIMIT ?")
                .bind(workspace.as_str()).bind(run.as_str()).bind(MAX_COMMANDS+1).fetch_all(&mut self.connection).await?;
            if commands.len() > MAX_COMMANDS as usize || commands.len() as i64 != row.accepted {
                return Err(invalid("recovery command high-water or limit mismatch").into());
            }
            if !unsupported && events.len() != commands.len() + 1 {
                return Err(ControlError::Conflict);
            }
            let mut pending_controls = Vec::new();
            for (n, c) in commands.iter().enumerate() {
                let seq = n as i64 + 1;
                if c.try_get::<i64, _>("seq")? != seq {
                    return Err(invalid("recovery command sequence gap").into());
                }
                let payload = DomainId::new(c.try_get::<String, _>("payload_blob_id")?)
                    .map_err(|_| invalid("missing command payload"))?;
                let bytes = self
                    .read_registered_blob(workspace, actor, &payload, COMMAND_MEDIA)
                    .await?;
                byte_count = byte_count
                    .checked_add(bytes.len())
                    .ok_or_else(|| invalid("recovery byte overflow"))?;
                if byte_count > MAX_BYTES {
                    return Err(invalid("recovery input byte limit exceeded").into());
                }
                let (command, canonical) = decode(&bytes)?;
                if bytes != canonical || control(&command)?.1 != &run {
                    return Err(ControlError::Conflict);
                }
                let original_receipt = self
                    .existing_command(workspace, actor, &command, &bytes)
                    .await?
                    .ok_or(ControlError::Conflict)?;
                let (command_id, _, key, kind, expected) = control(&command)?;
                if command_id.as_str() != c.try_get::<String, _>("id")?
                    || key != c.try_get::<String, _>("idempotency_key")?
                    || kind != c.try_get::<String, _>("kind")?
                    || expected != c.try_get::<Option<i64>, _>("expected_row_version")?
                    || c.try_get::<String, _>("actor_id")? != actor.as_str()
                    || c.try_get::<Vec<u8>, _>("digest")? != digest(&bytes)
                    || !matches!(&original_receipt, CommandReceipt::Accepted { accepted_seq, .. } if accepted_seq.value() as i64 == seq)
                {
                    return Err(ControlError::Conflict);
                }
                let state = c.try_get::<String, _>("state")?;
                if seq <= row.applied {
                    if state == "accepted" {
                        return Err(invalid("applied cursor covers unapplied control").into());
                    }
                    unsupported = true;
                } else if state != "accepted" {
                    unsupported = true;
                } else {
                    pending_controls.push(RecoveredControl {
                        sequence: positive(seq)?,
                        command,
                        canonical_bytes: bytes,
                        original_receipt,
                    });
                }
            }
            let has_attempt: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM local_attempts WHERE workspace_id=? AND run_id=?)",
            )
            .bind(workspace.as_str())
            .bind(run.as_str())
            .fetch_one(&mut self.connection)
            .await?;
            let checkpoint: Option<String> = sqlx::query_scalar(
                "SELECT current_checkpoint_id FROM local_runs WHERE workspace_id=? AND id=?",
            )
            .bind(workspace.as_str())
            .bind(run.as_str())
            .fetch_one(&mut self.connection)
            .await?;
            if let Some(id) = checkpoint {
                let id = DomainId::new(id).map_err(|_| ControlError::Conflict)?;
                self.read_checkpoint(workspace, actor, &run, &id).await?;
            }
            let extra:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM local_runs WHERE workspace_id=? AND id=? AND (session_id IS NOT NULL OR branch_id IS NOT NULL OR current_checkpoint_id IS NOT NULL))")
                .bind(workspace.as_str()).bind(run.as_str()).fetch_one(&mut self.connection).await?;
            let execution_intent:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM local_work_queue WHERE workspace_id=? AND run_id=? AND kind IN ('dispatch','wake'))")
                .bind(workspace.as_str()).bind(run.as_str()).fetch_one(&mut self.connection).await?;
            let gate = if matches!(row.status.as_str(), "completed" | "cancelled" | "failed") {
                RecoveryGate::Terminal
            } else if row.authority != "local"
                || row.device.as_deref() != Some(identity.device().as_str())
            {
                RecoveryGate::AuthorityHeld
            } else if row.cancelled || row.status == "cancelling" {
                RecoveryGate::CancellationPending
            } else if unsupported
                || has_attempt
                || extra
                || execution_intent
                || row.epoch != 0
                || row.applied != 0
                || !matches!(row.status.as_str(), "queued" | "paused")
            {
                RecoveryGate::ReconciliationRequired
            } else {
                RecoveryGate::AwaitingControlConsumer
            };
            runs.push(RecoveredRun {
                run,
                task: row.task,
                task_revision: row.revision,
                state,
                version: positive(row.version)?,
                epoch: counter(row.epoch)?,
                accepted: counter(row.accepted)?,
                applied: counter(row.applied)?,
                last_event: positive(row.event)?,
                gate,
                pending_controls,
            });
        }
        identity.check_live(&mut self.connection).await?;
        Ok(RecoverySnapshot { runs })
    }
}
