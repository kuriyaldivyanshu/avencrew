//! Atomic local control acceptance. Reconciliation intent never grants execution.
use super::blobs::{canonical, digest, invalid, relative};
use super::{LocalStore, StoreError};
use avencrew_contracts::{
    canonical_bytes,
    scalars::{Counter, DomainId, Instant, Nullable, PositiveCounter},
    wire::{Authority, Command, CommandReceipt, TerminalRunState},
    Validate,
};
use serde::{Deserialize, Serialize};
use sqlx::{Connection, Row, SqliteConnection};
use std::fmt;

pub(super) mod checkpoints;
mod recovery;
pub use checkpoints::{
    CheckpointFile, CheckpointPublication, CheckpointReceipt, FileCoverage, RestoredCheckpoint,
};
pub use recovery::{RecoveredControl, RecoveredRun, RecoveryGate, RecoverySnapshot};

const COMMAND_MEDIA: &str = "application/vnd.avencrew.control-command+json";
const EVENT_MEDIA: &str = "application/vnd.avencrew.control-accepted+json";
const RUN_MEDIA: &str = "application/vnd.avencrew.local-run-registration+json";
const MAX_COMMAND_BYTES: usize = 262_144;

#[derive(Debug)]
pub enum ControlError {
    Store(StoreError),
    InvalidCommand,
    Unsupported,
    NotFoundOrDenied,
    Conflict,
    IdempotencyMismatch,
    StaleAuthority,
    Cancelled,
}
impl fmt::Display for ControlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store(e) => e.fmt(f),
            Self::InvalidCommand => f.write_str("invalid command"),
            Self::Unsupported => f.write_str("command consumer is not enabled"),
            Self::NotFoundOrDenied => f.write_str("run unavailable to caller"),
            Self::Conflict => f.write_str("command version or identity conflict"),
            Self::IdempotencyMismatch => f.write_str("idempotency identity or digest mismatch"),
            Self::StaleAuthority => f.write_str("run does not have current local authority"),
            Self::Cancelled => f.write_str("run has sticky cancellation intent"),
        }
    }
}
impl std::error::Error for ControlError {}
impl From<StoreError> for ControlError {
    fn from(e: StoreError) -> Self {
        Self::Store(e)
    }
}
impl From<sqlx::Error> for ControlError {
    fn from(e: sqlx::Error) -> Self {
        Self::Store(e.into())
    }
}

/// Trusted storage registration only; not an executable RunSpec or budget grant.
pub struct LocalRunRegistration<'a> {
    pub workspace: &'a DomainId,
    pub actor: &'a DomainId,
    pub task: &'a DomainId,
    pub task_revision: &'a DomainId,
    pub run: &'a DomainId,
    pub event: &'a DomainId,
    pub queue: &'a DomainId,
    pub payload_blob: &'a DomainId,
    pub at: &'a Instant,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalRunReceipt {
    pub run_id: DomainId,
    pub task_id: DomainId,
    pub task_revision_id: DomainId,
    pub event_id: DomainId,
    pub queue_id: DomainId,
    pub payload_blob_id: DomainId,
}
#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct RegistrationData {
    schema_version: i32,
    workspace_id: DomainId,
    actor_id: DomainId,
    authority_device_id: DomainId,
    created_at: Instant,
    receipt: LocalRunReceipt,
}
/// Workspace/actor must come from the trusted supervisor, never client JSON.
pub struct CommandAcceptance<'a> {
    pub workspace: &'a DomainId,
    pub actor: &'a DomainId,
    pub bytes: &'a [u8],
    pub event: &'a DomainId,
    pub queue: &'a DomainId,
    pub command_blob: &'a DomainId,
    pub event_blob: &'a DomainId,
    pub at: &'a Instant,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AcceptedData {
    schema_version: i32,
    receipt: CommandReceipt,
}
struct RunRow {
    task: DomainId,
    revision: DomainId,
    authority: String,
    device: Option<String>,
    status: String,
    version: i64,
    epoch: i64,
    accepted: i64,
    applied: i64,
    event: i64,
    cancelled: bool,
}
async fn run_row(
    conn: &mut SqliteConnection,
    workspace: &DomainId,
    run: &DomainId,
    actor: &DomainId,
) -> Result<RunRow, ControlError> {
    let row=sqlx::query("SELECT r.* FROM local_runs r JOIN local_tasks t ON t.workspace_id=r.workspace_id AND t.id=r.task_id WHERE r.workspace_id=? AND r.id=? AND t.owner_actor_id=?").bind(workspace.as_str()).bind(run.as_str()).bind(actor.as_str()).fetch_optional(conn).await?.ok_or(ControlError::NotFoundOrDenied)?;
    let id = |name| -> Result<DomainId, ControlError> {
        DomainId::new(row.try_get::<String, _>(name)?)
            .map_err(|_| invalid("invalid retained run identity").into())
    };
    Ok(RunRow {
        task: id("task_id")?,
        revision: id("task_revision_id")?,
        authority: row.try_get("authority")?,
        device: row.try_get("authority_device_id")?,
        status: row.try_get("status")?,
        version: row.try_get("row_version")?,
        epoch: row.try_get("fencing_epoch")?,
        accepted: row.try_get("accepted_command_seq")?,
        applied: row.try_get("applied_command_seq")?,
        event: row.try_get("last_event_seq")?,
        cancelled: row.try_get::<i64, _>("cancellation_requested")? == 1,
    })
}
fn require_local(run: &RunRow, device: &DomainId) -> Result<(), ControlError> {
    if run.authority != "local" || run.device.as_deref() != Some(device.as_str()) {
        return Err(ControlError::StaleAuthority);
    }
    Ok(())
}
fn next(n: i64) -> Result<i64, ControlError> {
    n.checked_add(1)
        .filter(|v| *v > 0)
        .ok_or_else(|| invalid("run cursor overflow").into())
}
fn counter(n: i64) -> Result<Counter, ControlError> {
    Counter::new(n.to_string()).map_err(|_| invalid("invalid retained run cursor").into())
}
fn positive(n: i64) -> Result<PositiveCounter, ControlError> {
    PositiveCounter::new(n.to_string()).map_err(|_| invalid("invalid retained run version").into())
}
type ControlParts<'a> = (&'a DomainId, &'a DomainId, &'a str, &'a str, Option<i64>);
fn control(command: &Command) -> Result<ControlParts<'_>, ControlError> {
    Ok(match command {
        Command::Steer {
            command_id,
            run_id,
            idempotency_key,
            payload,
        } => (
            command_id,
            run_id,
            idempotency_key.as_str(),
            "steer",
            Some(payload.expected_revision.value() as i64),
        ),
        Command::Pause {
            command_id,
            run_id,
            idempotency_key,
            payload,
        } => (
            command_id,
            run_id,
            idempotency_key.as_str(),
            "pause",
            Some(payload.expected_revision.value() as i64),
        ),
        Command::Stop {
            command_id,
            run_id,
            idempotency_key,
            ..
        } => (command_id, run_id, idempotency_key.as_str(), "cancel", None),
        _ => return Err(ControlError::Unsupported),
    })
}
fn decode(bytes: &[u8]) -> Result<(Command, Vec<u8>), ControlError> {
    if bytes.len() > MAX_COMMAND_BYTES {
        return Err(ControlError::InvalidCommand);
    }
    let canonical = canonical_bytes(bytes).map_err(|_| ControlError::InvalidCommand)?;
    let command: Command =
        serde_json::from_slice(&canonical).map_err(|_| ControlError::InvalidCommand)?;
    command
        .validate()
        .map_err(|_| ControlError::InvalidCommand)?;
    Ok((command, canonical))
}
async fn blob(
    conn: &mut SqliteConnection,
    w: &DomainId,
    a: &DomainId,
    id: &DomainId,
    bytes: &[u8],
    media: &str,
    at: i64,
) -> Result<(), ControlError> {
    let existing = sqlx::query("SELECT * FROM local_blobs WHERE workspace_id=? AND id=?")
        .bind(w.as_str())
        .bind(id.as_str())
        .fetch_optional(&mut *conn)
        .await?;
    if let Some(row) = existing {
        if row.try_get::<String, _>("storage_relpath")? != relative(w, id)
            || row.try_get::<Vec<u8>, _>("sha256")? != digest(bytes)
            || row.try_get::<i64, _>("byte_size")? != bytes.len() as i64
            || row.try_get::<String, _>("owner_actor_id")? != a.as_str()
            || row.try_get::<String, _>("media_type")? != media
            || row.try_get::<String, _>("status")? != "verified"
            || row.try_get::<Option<i64>, _>("erased_at_us")?.is_some()
        {
            return Err(ControlError::Conflict);
        }
        return Ok(());
    }
    sqlx::query("INSERT INTO local_blobs (id,workspace_id,created_at_us,owner_actor_id,storage_relpath,sha256,byte_size,media_type,status,erased_at_us) VALUES (?,?,?,?,?,?,?,?,'verified',NULL)").bind(id.as_str()).bind(w.as_str()).bind(at).bind(a.as_str()).bind(relative(w,id)).bind(digest(bytes).as_slice()).bind(bytes.len() as i64).bind(media).execute(conn).await?;
    Ok(())
}
struct JournalEvent<'a> {
    w: &'a DomainId,
    run: &'a DomainId,
    id: &'a DomainId,
    seq: i64,
    epoch: i64,
    kind: &'a str,
    command: Option<&'a DomainId>,
    payload: &'a DomainId,
    at: i64,
}
async fn event(conn: &mut SqliteConnection, event: JournalEvent<'_>) -> Result<(), ControlError> {
    let JournalEvent {
        w,
        run,
        id,
        seq,
        epoch,
        kind,
        command,
        payload,
        at,
    } = event;
    sqlx::query("INSERT INTO local_events (id,workspace_id,created_at_us,run_id,seq,attempt_id,epoch,event_type,schema_version,occurred_at_us,payload_blob_id,command_id) VALUES (?,?,?,?,?,NULL,?,?,1,?,?,?)").bind(id.as_str()).bind(w.as_str()).bind(at).bind(run.as_str()).bind(seq).bind(epoch).bind(kind).bind(at).bind(payload.as_str()).bind(command.map(DomainId::as_str)).execute(conn).await?;
    Ok(())
}
async fn queue(
    conn: &mut SqliteConnection,
    w: &DomainId,
    run: &DomainId,
    id: &DomainId,
    key: &str,
    payload: &DomainId,
    at: i64,
) -> Result<(), ControlError> {
    sqlx::query("INSERT INTO local_work_queue (id,workspace_id,created_at_us,run_id,kind,dedupe_key,due_at_us,state,claim_token,lease_until_us,attempt_count,spec_blob_id,error_code) VALUES (?,?,?,?,'reconcile',?,?,'ready',NULL,NULL,0,?,NULL)").bind(id.as_str()).bind(w.as_str()).bind(at).bind(run.as_str()).bind(key).bind(at).bind(payload.as_str()).execute(conn).await?;
    Ok(())
}
impl LocalStore {
    pub async fn register_local_run(
        &mut self,
        input: LocalRunRegistration<'_>,
    ) -> Result<LocalRunReceipt, ControlError> {
        let identity = self.verified_identity(input.workspace, input.actor).await?;
        let task = self
            .read_task_bundle(input.workspace, input.actor, input.task)
            .await?;
        if &task.receipt.version_id != input.task_revision {
            return Err(ControlError::Conflict);
        }
        let receipt = LocalRunReceipt {
            run_id: input.run.clone(),
            task_id: input.task.clone(),
            task_revision_id: input.task_revision.clone(),
            event_id: input.event.clone(),
            queue_id: input.queue.clone(),
            payload_blob_id: input.payload_blob.clone(),
        };
        let data = RegistrationData {
            schema_version: 1,
            workspace_id: input.workspace.clone(),
            actor_id: input.actor.clone(),
            authority_device_id: identity.device().clone(),
            created_at: input.at.clone(),
            receipt: receipt.clone(),
        };
        let bytes = canonical(&data)?;
        if sqlx::query("SELECT 1 FROM local_runs WHERE workspace_id=? AND id=?")
            .bind(input.workspace.as_str())
            .bind(input.run.as_str())
            .fetch_optional(&mut self.connection)
            .await?
            .is_some()
        {
            let row = run_row(
                &mut self.connection,
                input.workspace,
                input.run,
                input.actor,
            )
            .await?;
            if &row.task != input.task || &row.revision != input.task_revision {
                return Err(ControlError::Conflict);
            }
            let saved = self
                .read_registered_blob(input.workspace, input.actor, input.payload_blob, RUN_MEDIA)
                .await?;
            if saved != bytes {
                return Err(ControlError::Conflict);
            }
            verify_intent(
                &mut self.connection,
                input.workspace,
                input.run,
                input.queue,
                &format!("run:{}", input.run.as_str()),
                input.payload_blob,
                input.at.unix_micros(),
            )
            .await?;
            verify_event(
                &mut self.connection,
                JournalEvent {
                    w: input.workspace,
                    run: input.run,
                    id: input.event,
                    seq: 1,
                    epoch: 0,
                    kind: "run.accepted",
                    command: None,
                    payload: input.payload_blob,
                    at: input.at.unix_micros(),
                },
            )
            .await?;
            identity.check_live(&mut self.connection).await?;
            return Ok(receipt);
        }
        if identity.blob_ids().any(|id| id == input.payload_blob) {
            return Err(ControlError::Conflict);
        }
        self.put_canonical(input.workspace, input.payload_blob, &bytes)?;
        let mut tx = self.connection.begin().await?;
        identity.check_live(&mut tx).await?;
        blob(
            &mut tx,
            input.workspace,
            input.actor,
            input.payload_blob,
            &bytes,
            RUN_MEDIA,
            input.at.unix_micros(),
        )
        .await?;
        sqlx::query("INSERT INTO local_runs (id,workspace_id,created_at_us,task_id,task_revision_id,session_id,authority,authority_device_id,status,row_version,fencing_epoch,accepted_command_seq,applied_command_seq,last_event_seq,current_checkpoint_id,cancellation_requested,branch_id) VALUES (?,?,?,?,?,NULL,'local',?,'queued',1,0,0,0,1,NULL,0,NULL)").bind(input.run.as_str()).bind(input.workspace.as_str()).bind(input.at.unix_micros()).bind(input.task.as_str()).bind(input.task_revision.as_str()).bind(identity.device().as_str()).execute(&mut *tx).await?;
        event(
            &mut tx,
            JournalEvent {
                w: input.workspace,
                run: input.run,
                id: input.event,
                seq: 1,
                epoch: 0,
                kind: "run.accepted",
                command: None,
                payload: input.payload_blob,
                at: input.at.unix_micros(),
            },
        )
        .await?;
        queue(
            &mut tx,
            input.workspace,
            input.run,
            input.queue,
            &format!("run:{}", input.run.as_str()),
            input.payload_blob,
            input.at.unix_micros(),
        )
        .await?;
        #[cfg(test)]
        commit_barrier(&self.status.database_path, "before_commit")?;
        tx.commit().await?;
        #[cfg(test)]
        commit_barrier(&self.status.database_path, "after_commit")?;
        Ok(receipt)
    }
    pub async fn accept_command(
        &mut self,
        input: CommandAcceptance<'_>,
    ) -> Result<CommandReceipt, ControlError> {
        let (command, bytes) = decode(input.bytes)?;
        let (id, run, key, kind, expected) = control(&command)?;
        let identity = self.verified_identity(input.workspace, input.actor).await?;
        let row = run_row(&mut self.connection, input.workspace, run, input.actor).await?;
        let task = self
            .read_task_bundle(input.workspace, input.actor, &row.task)
            .await?;
        if task.receipt.version_id != row.revision {
            return Err(ControlError::Conflict);
        }
        if let Some(receipt) = self
            .existing_command(input.workspace, input.actor, &command, &bytes)
            .await?
        {
            identity.check_live(&mut self.connection).await?;
            return Ok(receipt);
        }
        require_local(&row, identity.device())?;
        let terminal = match row.status.as_str() {
            "completed" => Some(TerminalRunState::Completed),
            "cancelled" => Some(TerminalRunState::Cancelled),
            "failed" => Some(TerminalRunState::Failed),
            _ => None,
        };
        if let Some(state) = terminal {
            identity.check_live(&mut self.connection).await?;
            return Ok(CommandReceipt::Terminal {
                command_id: id.clone(),
                run_id: run.clone(),
                state,
                row_version: positive(row.version)?,
            });
        }
        if kind != "cancel" && row.cancelled {
            return Err(ControlError::Cancelled);
        }
        if expected.is_some_and(|v| v != row.version) {
            return Err(ControlError::Conflict);
        }
        let seq = next(row.accepted)?;
        let event_seq = next(row.event)?;
        let version = next(row.version)?;
        let receipt = CommandReceipt::Accepted {
            command_id: id.clone(),
            authority: Authority::Local,
            run_id: run.clone(),
            accepted_seq: counter(seq)?,
            event_seq: counter(event_seq)?,
            row_version: positive(version)?,
            disposition_ref: Nullable(Some(input.event_blob.clone())),
            task_id: row.task.clone(),
            task_revision_id: row.revision.clone(),
        };
        let event_bytes = canonical(&AcceptedData {
            schema_version: 1,
            receipt: receipt.clone(),
        })?;
        if input.command_blob == input.event_blob
            || identity
                .blob_ids()
                .any(|i| i == input.command_blob || i == input.event_blob)
        {
            return Err(ControlError::Conflict);
        }
        self.put_canonical(input.workspace, input.command_blob, &bytes)?;
        self.put_canonical(input.workspace, input.event_blob, &event_bytes)?;
        let mut tx = self.connection.begin().await?;
        identity.check_live(&mut tx).await?;
        let current = run_row(&mut tx, input.workspace, run, input.actor).await?;
        require_local(&current, identity.device())?;
        if current.version != row.version
            || current.accepted != row.accepted
            || current.event != row.event
            || current.applied != row.applied
            || current.revision != row.revision
            || current.task != row.task
            || current.status != row.status
            || current.epoch != row.epoch
            || current.cancelled != row.cancelled
        {
            return Err(ControlError::Conflict);
        }
        if sqlx::query("SELECT 1 FROM local_commands WHERE workspace_id=? AND (id=? OR (run_id=? AND idempotency_key=?))").bind(input.workspace.as_str()).bind(id.as_str()).bind(run.as_str()).bind(key).fetch_optional(&mut *tx).await?.is_some() { return Err(ControlError::Conflict); }
        blob(
            &mut tx,
            input.workspace,
            input.actor,
            input.command_blob,
            &bytes,
            COMMAND_MEDIA,
            input.at.unix_micros(),
        )
        .await?;
        blob(
            &mut tx,
            input.workspace,
            input.actor,
            input.event_blob,
            &event_bytes,
            EVENT_MEDIA,
            input.at.unix_micros(),
        )
        .await?;
        sqlx::query("INSERT INTO local_commands (id,workspace_id,created_at_us,run_id,seq,actor_id,kind,idempotency_key,digest,payload_blob_id,expected_row_version,state) VALUES (?,?,?,?,?,?,?,?,?,?,?,'accepted')").bind(id.as_str()).bind(input.workspace.as_str()).bind(input.at.unix_micros()).bind(run.as_str()).bind(seq).bind(input.actor.as_str()).bind(kind).bind(key).bind(digest(&bytes).as_slice()).bind(input.command_blob.as_str()).bind(expected).execute(&mut *tx).await?;
        event(
            &mut tx,
            JournalEvent {
                w: input.workspace,
                run,
                id: input.event,
                seq: event_seq,
                epoch: row.epoch,
                kind: "control.accepted",
                command: Some(id),
                payload: input.event_blob,
                at: input.at.unix_micros(),
            },
        )
        .await?;
        let changed = sqlx::query("UPDATE local_runs SET accepted_command_seq=?,last_event_seq=?,row_version=?,cancellation_requested=?,status=? WHERE workspace_id=? AND id=? AND row_version=?").bind(seq).bind(event_seq).bind(version).bind(if kind=="cancel" {1}else{i64::from(row.cancelled)}).bind(if kind=="cancel" {"cancelling"}else{&row.status}).bind(input.workspace.as_str()).bind(run.as_str()).bind(row.version).execute(&mut *tx).await?.rows_affected();
        if changed != 1 {
            return Err(ControlError::Conflict);
        }
        if kind == "cancel" {
            sqlx::query("UPDATE local_work_queue SET state='cancelled',claim_token=NULL,lease_until_us=NULL WHERE workspace_id=? AND run_id=? AND kind IN ('dispatch','wake') AND state IN ('ready','claimed')").bind(input.workspace.as_str()).bind(run.as_str()).execute(&mut *tx).await?;
        }
        queue(
            &mut tx,
            input.workspace,
            run,
            input.queue,
            &format!("command:{}", id.as_str()),
            input.command_blob,
            input.at.unix_micros(),
        )
        .await?;
        #[cfg(test)]
        commit_barrier(&self.status.database_path, "before_commit")?;
        tx.commit().await?;
        #[cfg(test)]
        commit_barrier(&self.status.database_path, "after_commit")?;
        Ok(receipt)
    }
    async fn existing_command(
        &mut self,
        w: &DomainId,
        a: &DomainId,
        command: &Command,
        bytes: &[u8],
    ) -> Result<Option<CommandReceipt>, ControlError> {
        let (id, run, key, kind, expected) = control(command)?;
        let found = sqlx::query(
            "SELECT * FROM local_commands WHERE workspace_id=? AND run_id=? AND idempotency_key=?",
        )
        .bind(w.as_str())
        .bind(run.as_str())
        .bind(key)
        .fetch_optional(&mut self.connection)
        .await?;
        let Some(c) = found else {
            if sqlx::query("SELECT 1 FROM local_commands WHERE workspace_id=? AND id=?")
                .bind(w.as_str())
                .bind(id.as_str())
                .fetch_optional(&mut self.connection)
                .await?
                .is_some()
            {
                return Err(ControlError::Conflict);
            }
            return Ok(None);
        };
        if c.try_get::<String, _>("id")? != id.as_str()
            || c.try_get::<Vec<u8>, _>("digest")? != digest(bytes)
        {
            return Err(ControlError::IdempotencyMismatch);
        }
        if c.try_get::<String, _>("actor_id")? != a.as_str()
            || c.try_get::<String, _>("kind")? != kind
            || c.try_get::<Option<i64>, _>("expected_row_version")? != expected
        {
            return Err(ControlError::Conflict);
        }
        let payload = DomainId::new(c.try_get::<String, _>("payload_blob_id")?)
            .map_err(|_| invalid("invalid command payload reference"))?;
        if self
            .read_registered_blob(w, a, &payload, COMMAND_MEDIA)
            .await?
            != bytes
        {
            return Err(ControlError::Conflict);
        }
        let e=sqlx::query("SELECT * FROM local_events WHERE workspace_id=? AND run_id=? AND command_id=? AND event_type='control.accepted' LIMIT 2").bind(w.as_str()).bind(run.as_str()).bind(id.as_str()).fetch_all(&mut self.connection).await?;
        if e.len() != 1 {
            return Err(ControlError::Conflict);
        }
        let e = &e[0];
        let event_blob = DomainId::new(e.try_get::<String, _>("payload_blob_id")?)
            .map_err(|_| invalid("invalid event payload reference"))?;
        let data_bytes = self
            .read_registered_blob(w, a, &event_blob, EVENT_MEDIA)
            .await?;
        let data: AcceptedData =
            serde_json::from_slice(&data_bytes).map_err(|_| invalid("invalid accepted receipt"))?;
        data.receipt
            .validate()
            .map_err(|_| invalid("invalid accepted receipt"))?;
        if data.schema_version != 1 || canonical(&data)? != data_bytes {
            return Err(ControlError::Conflict);
        }
        let CommandReceipt::Accepted {
            command_id,
            authority,
            run_id,
            accepted_seq,
            event_seq,
            row_version,
            disposition_ref,
            task_id,
            task_revision_id,
        } = &data.receipt
        else {
            return Err(ControlError::Conflict);
        };
        let current = run_row(&mut self.connection, w, run, a).await?;
        if command_id != id
            || authority != &Authority::Local
            || run_id != run
            || accepted_seq.value() as i64 != c.try_get::<i64, _>("seq")?
            || event_seq.value() as i64 != e.try_get::<i64, _>("seq")?
            || row_version.value() as i64 > current.version
            || row_version.value() < 2
            || accepted_seq.value() as i64 > current.accepted
            || event_seq.value() as i64 > current.event
            || disposition_ref.0.as_ref() != Some(&event_blob)
            || task_id != &current.task
            || task_revision_id != &current.revision
        {
            return Err(ControlError::Conflict);
        }
        let event_id = DomainId::new(e.try_get::<String, _>("id")?)
            .map_err(|_| invalid("invalid event identity"))?;
        let at = c.try_get::<i64, _>("created_at_us")?;
        verify_event(
            &mut self.connection,
            JournalEvent {
                w,
                run,
                id: &event_id,
                seq: event_seq.value() as i64,
                epoch: e.try_get("epoch")?,
                kind: "control.accepted",
                command: Some(id),
                payload: &event_blob,
                at,
            },
        )
        .await?;
        let q=sqlx::query("SELECT id FROM local_work_queue WHERE workspace_id=? AND kind='reconcile' AND dedupe_key=?").bind(w.as_str()).bind(format!("command:{}",id.as_str())).fetch_optional(&mut self.connection).await?.ok_or(ControlError::Conflict)?;
        let queue_id = DomainId::new(q.try_get::<String, _>("id")?)
            .map_err(|_| invalid("invalid queue identity"))?;
        verify_intent(
            &mut self.connection,
            w,
            run,
            &queue_id,
            &format!("command:{}", id.as_str()),
            &payload,
            at,
        )
        .await?;
        Ok(Some(data.receipt))
    }
}
async fn verify_intent(
    conn: &mut SqliteConnection,
    w: &DomainId,
    run: &DomainId,
    id: &DomainId,
    key: &str,
    payload: &DomainId,
    at: i64,
) -> Result<(), ControlError> {
    let q = sqlx::query("SELECT * FROM local_work_queue WHERE workspace_id=? AND id=?")
        .bind(w.as_str())
        .bind(id.as_str())
        .fetch_optional(conn)
        .await?
        .ok_or(ControlError::Conflict)?;
    if q.try_get::<String, _>("run_id")? != run.as_str()
        || q.try_get::<String, _>("kind")? != "reconcile"
        || q.try_get::<String, _>("dedupe_key")? != key
        || q.try_get::<String, _>("spec_blob_id")? != payload.as_str()
        || q.try_get::<i64, _>("created_at_us")? != at
    {
        return Err(ControlError::Conflict);
    }
    Ok(())
}
async fn verify_event(
    conn: &mut SqliteConnection,
    event: JournalEvent<'_>,
) -> Result<(), ControlError> {
    let JournalEvent {
        w,
        run,
        id,
        seq,
        epoch,
        kind,
        command,
        payload,
        at,
    } = event;
    let e = sqlx::query("SELECT * FROM local_events WHERE workspace_id=? AND id=?")
        .bind(w.as_str())
        .bind(id.as_str())
        .fetch_optional(conn)
        .await?
        .ok_or(ControlError::Conflict)?;
    if e.try_get::<String, _>("run_id")? != run.as_str()
        || e.try_get::<i64, _>("seq")? != seq
        || e.try_get::<i64, _>("epoch")? != epoch
        || e.try_get::<String, _>("event_type")? != kind
        || e.try_get::<i64, _>("schema_version")? != 1
        || e.try_get::<i64, _>("created_at_us")? != at
        || e.try_get::<i64, _>("occurred_at_us")? != at
        || e.try_get::<Option<String>, _>("command_id")?.as_deref() != command.map(DomainId::as_str)
        || e.try_get::<String, _>("payload_blob_id")? != payload.as_str()
        || e.try_get::<Option<String>, _>("attempt_id")?.is_some()
    {
        return Err(ControlError::Conflict);
    }
    Ok(())
}
#[cfg(test)]
pub(super) mod tests;
#[cfg(test)]
fn commit_barrier(database: &std::path::Path, stage: &str) -> Result<(), StoreError> {
    if std::env::var("AVENCREW_CONTROL_CRASH_STAGE").as_deref() != Ok(stage)
        || std::env::var("AVENCREW_CONTROL_CRASH_ROOT").ok().as_deref()
            != database.parent().and_then(|p| p.to_str())
    {
        return Ok(());
    }
    let root = database.parent().unwrap();
    std::fs::write(root.join("control-crash-pending"), stage)?;
    std::fs::rename(
        root.join("control-crash-pending"),
        root.join("control-crash-ready"),
    )?;
    let started = std::time::Instant::now();
    while started.elapsed() < std::time::Duration::from_secs(30) {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    Err(invalid("control crash test timed out"))
}
