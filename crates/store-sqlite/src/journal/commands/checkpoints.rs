//! Exact sealed checkpoint inventory; restoration is not execution admission.
use super::super::blobs::hex;
use super::*;
use avencrew_contracts::bundles::{BundleCandidate, Purpose};
use std::{collections::BTreeSet, fs::File};

const MEDIA: &str = "application/vnd.avencrew.checkpoint-bundle+json";
const COMMIT_MEDIA: &str = "application/vnd.avencrew.checkpoint-committed+json";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Checkpoint {
    id: DomainId,
    workspace_id: DomainId,
    created_at: Instant,
    run_id: DomainId,
    attempt_id: DomainId,
    event_seq: Counter,
    accepted_command_seq: Counter,
    applied_command_seq: Counter,
    manifest_id: DomainId,
    context_manifest_id: Nullable<DomainId>,
    checkpoint_kind: String,
    schema_version: i32,
    digest: avencrew_contracts::scalars::Digest,
    sealed_at: Instant,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    id: DomainId,
    workspace_id: DomainId,
    created_at: Instant,
    owner_resource_id: DomainId,
    kind: String,
    schema_version: i32,
    digest: avencrew_contracts::scalars::Digest,
    sealed_at: Instant,
    deletion_generation: Counter,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    id: DomainId,
    workspace_id: DomainId,
    created_at: Instant,
    manifest_id: DomainId,
    entry_key: String,
    kind: String,
    blob_id: Nullable<DomainId>,
    resource_version_id: Nullable<DomainId>,
    invocation_id: Nullable<DomainId>,
    process_id: Nullable<DomainId>,
    child_run_id: Nullable<DomainId>,
    credential_reference_id: Nullable<DomainId>,
    sha256: Nullable<avencrew_contracts::scalars::Digest>,
    byte_size: Nullable<Counter>,
    mode_bits: Nullable<i32>,
    disposition: String,
    metadata: serde_json::Value,
}
/// Explicitly incomplete recovery coverage. No complete tree/context claim.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileCoverage {
    pub schema_version: String,
    pub capture: String,
    pub filesystem_event_seq: Counter,
    pub gaps: Vec<String>,
    pub context: String,
    pub effects: String,
    pub processes: String,
    pub attempt_epoch: PositiveCounter,
    pub harness_build_digest: avencrew_contracts::scalars::Digest,
    pub protocol_version: i32,
}
pub struct CheckpointPublication<'a> {
    pub workspace: &'a DomainId,
    pub actor: &'a DomainId,
    pub bytes: &'a [u8],
    pub sha256: [u8; 32],
    pub bundle_blob: &'a DomainId,
    pub event: &'a DomainId,
    pub event_blob: &'a DomainId,
    pub expected_version: &'a PositiveCounter,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointReceipt {
    pub checkpoint_id: DomainId,
    pub run_id: DomainId,
    pub attempt_id: DomainId,
    pub manifest_id: DomainId,
    pub bundle_blob_id: DomainId,
    pub event_id: DomainId,
    pub event_blob_id: DomainId,
    pub event_seq: PositiveCounter,
    pub row_version: PositiveCounter,
    pub bundle_digest: avencrew_contracts::scalars::Digest,
}
pub struct CheckpointFile {
    pub path: String,
    pub mode_bits: u32,
    pub file: File,
}
pub struct RestoredCheckpoint {
    pub receipt: CheckpointReceipt,
    pub canonical_bytes: Vec<u8>,
    pub coverage: FileCoverage,
    pub files: Vec<CheckpointFile>,
}
struct Closure {
    candidate: BundleCandidate,
    checkpoint: Checkpoint,
    manifest: Manifest,
    entries: Vec<Entry>,
    coverage: FileCoverage,
}
fn decode_checkpoint(bytes: &[u8], w: &DomainId) -> Result<Closure, ControlError> {
    let candidate =
        BundleCandidate::inspect(bytes, w).map_err(|_| invalid("invalid checkpoint bundle"))?;
    if candidate.purpose() != Purpose::Checkpoint || candidate.records().len() > 67 {
        return Err(ControlError::Unsupported);
    }
    let mut checkpoint = None;
    let mut manifest = None;
    let mut entries = Vec::new();
    let mut ids = BTreeSet::new();
    for r in candidate.records() {
        if !ids.insert((r.table.clone(), r.id.clone())) {
            return Err(ControlError::Conflict);
        }
        match r.table.as_str() {
            "checkpoints" if checkpoint.is_none() => {
                checkpoint = Some(
                    serde_json::from_value::<Checkpoint>(r.record.clone())
                        .map_err(|_| invalid("invalid checkpoint record"))?,
                )
            }
            "manifests" if manifest.is_none() => {
                manifest = Some(
                    serde_json::from_value::<Manifest>(r.record.clone())
                        .map_err(|_| invalid("invalid manifest record"))?,
                )
            }
            "manifest_entries" => entries.push(
                serde_json::from_value::<Entry>(r.record.clone())
                    .map_err(|_| invalid("invalid manifest entry"))?,
            ),
            _ => return Err(ControlError::Unsupported),
        }
    }
    let c = checkpoint.ok_or(ControlError::Conflict)?;
    let m = manifest.ok_or(ControlError::Conflict)?;
    entries.sort_by(|a, b| a.entry_key.cmp(&b.entry_key));
    if candidate.root().id != c.id
        || m.id != c.manifest_id
        || c.schema_version != 1
        || m.schema_version != 1
        || m.kind != "checkpoint"
        || m.deletion_generation.value() != 0
        || c.context_manifest_id.0.is_some()
        || !matches!(c.checkpoint_kind.as_str(), "logical" | "filesystem")
        || c.created_at != m.created_at
        || c.sealed_at != m.sealed_at
        || c.sealed_at.unix_micros() < c.created_at.unix_micros()
        || c.applied_command_seq.value() > c.accepted_command_seq.value()
    {
        return Err(ControlError::Unsupported);
    }
    if hex(&digest(&canonical(&entries)?)) != m.digest.as_str() {
        return Err(ControlError::Conflict);
    }
    let mut v = serde_json::to_value(&c).map_err(|_| invalid("checkpoint encoding"))?;
    v.as_object_mut()
        .ok_or(ControlError::Conflict)?
        .remove("digest");
    if hex(&digest(&canonical(
        &serde_json::json!({"domain":"avencrew.local-checkpoint-record/1","record":v,"manifest_digest":m.digest}),
    )?)) != c.digest.as_str()
    {
        return Err(ControlError::Conflict);
    }
    let mut keys = BTreeSet::new();
    let mut coverage = None;
    let mut blobs = BTreeSet::new();
    let mut total = 0u64;
    for e in &entries {
        if e.manifest_id != m.id
            || e.created_at != m.created_at
            || !keys.insert(e.entry_key.as_str())
            || e.resource_version_id.0.is_some()
            || e.invocation_id.0.is_some()
            || e.process_id.0.is_some()
            || e.child_run_id.0.is_some()
            || e.credential_reference_id.0.is_some()
        {
            return Err(ControlError::Unsupported);
        }
        if e.entry_key == "coverage" {
            if e.kind != "metadata"
                || e.disposition != "declared"
                || e.blob_id.0.is_some()
                || e.sha256.0.is_some()
                || e.byte_size.0.is_some()
                || e.mode_bits.0.is_some()
            {
                return Err(ControlError::Conflict);
            }
            coverage = Some(
                serde_json::from_value::<FileCoverage>(e.metadata.clone())
                    .map_err(|_| invalid("invalid checkpoint coverage"))?,
            );
        } else {
            let path = e
                .entry_key
                .strip_prefix("file/")
                .ok_or(ControlError::Unsupported)?;
            if path.is_empty()
                || path.len() > 4096
                || path.contains(['\\', '\0', ':'])
                || path
                    .split('/')
                    .any(|p| p.is_empty() || p == "." || p == "..")
                || e.kind != "file"
                || e.disposition != "captured"
                || e.metadata != serde_json::json!({})
                || e.mode_bits.0.is_none_or(|n| n < 0 || n & !0o777 != 0)
            {
                return Err(ControlError::Unsupported);
            }
            let id = e.blob_id.0.as_ref().ok_or(ControlError::Conflict)?;
            let hash = e.sha256.0.as_ref().ok_or(ControlError::Conflict)?;
            let size = e.byte_size.0.as_ref().ok_or(ControlError::Conflict)?;
            total = total
                .checked_add(size.value())
                .ok_or(ControlError::Conflict)?;
            if total > 256 * 1024 * 1024 {
                return Err(invalid("checkpoint raw byte limit").into());
            }
            blobs.insert((id.clone(), hash.clone(), size.clone()));
        }
    }
    let coverage = coverage.ok_or(ControlError::Conflict)?;
    let files = entries.len().checked_sub(1).ok_or(ControlError::Conflict)?;
    if coverage.schema_version != "avencrew.checkpoint-coverage/1"
        || coverage.context != "not_captured"
        || coverage.effects != "unverified"
        || coverage.processes != "unverified"
        || coverage.protocol_version != 1
        || coverage.gaps.is_empty()
        || coverage.gaps.len() > 128
        || coverage.gaps.iter().any(|g| g.is_empty() || g.len() > 4096)
        || coverage.filesystem_event_seq.value() > c.event_seq.value()
        || !matches!(
            (coverage.capture.as_str(), c.checkpoint_kind.as_str(), files),
            ("none", "logical", 0) | ("per_file", "filesystem", 1..=64)
        )
    {
        return Err(ControlError::Unsupported);
    }
    let refs: BTreeSet<_> = candidate
        .referenced_blobs()
        .iter()
        .map(|b| (b.id.clone(), b.sha256.clone(), b.byte_size.clone()))
        .collect();
    if refs != blobs || refs.len() != candidate.referenced_blobs().len() {
        return Err(ControlError::Conflict);
    }
    Ok(Closure {
        candidate,
        checkpoint: c,
        manifest: m,
        entries,
        coverage,
    })
}

async fn attempt(
    conn: &mut SqliteConnection,
    w: &DomainId,
    c: &Closure,
    active: bool,
) -> Result<(), ControlError> {
    let row =
        sqlx::query("SELECT * FROM local_attempts WHERE workspace_id=? AND id=? AND run_id=?")
            .bind(w.as_str())
            .bind(c.checkpoint.attempt_id.as_str())
            .bind(c.checkpoint.run_id.as_str())
            .fetch_optional(conn)
            .await?
            .ok_or(ControlError::Conflict)?;
    if row.try_get::<i64, _>("started_at_us")? > c.checkpoint.created_at.unix_micros()
        || row.try_get::<String, _>("placement")? != "local"
        || row.try_get::<i64, _>("epoch")? != c.coverage.attempt_epoch.value() as i64
        || row.try_get::<String, _>("harness_build_digest")?
            != c.coverage.harness_build_digest.as_str()
        || row.try_get::<i64, _>("protocol_version")? != i64::from(c.coverage.protocol_version)
        || (active
            && (row.try_get::<Option<i64>, _>("ended_at_us")?.is_some()
                || !matches!(
                    row.try_get::<String, _>("state")?.as_str(),
                    "claimed" | "running" | "draining"
                )))
    {
        return Err(ControlError::Conflict);
    }
    Ok(())
}
fn digest_bytes(s: &str) -> Vec<u8> {
    s.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| {
            fn n(c: u8) -> u8 {
                if c <= b'9' {
                    c - b'0'
                } else {
                    c - b'a' + 10
                }
            }
            n(p[0]) * 16 + n(p[1])
        })
        .collect()
}
fn cut(row: &RunRow, c: &Closure, version: &PositiveCounter) -> Result<(), ControlError> {
    if row.cancelled || row.status == "cancelling" {
        return Err(ControlError::Cancelled);
    }
    if !matches!(
        row.status.as_str(),
        "preparing" | "running" | "waiting" | "paused" | "verifying" | "recovering"
    ) || row.version != version.value() as i64
        || row.epoch != c.coverage.attempt_epoch.value() as i64
        || row.event != c.checkpoint.event_seq.value() as i64
        || row.accepted != c.checkpoint.accepted_command_seq.value() as i64
        || row.applied != c.checkpoint.applied_command_seq.value() as i64
    {
        return Err(ControlError::Conflict);
    }
    Ok(())
}
impl LocalStore {
    async fn checkpoint_files(
        &mut self,
        w: &DomainId,
        a: &DomainId,
        c: &Closure,
    ) -> Result<Vec<CheckpointFile>, ControlError> {
        let mut files = Vec::new();
        for e in &c.entries {
            if e.entry_key == "coverage" {
                continue;
            }
            let id = e.blob_id.0.as_ref().ok_or(ControlError::Conflict)?;
            let file = self.open_vault_blob(w, a, id).await?;
            let row = sqlx::query(
                "SELECT sha256,byte_size FROM local_blobs WHERE workspace_id=? AND id=?",
            )
            .bind(w.as_str())
            .bind(id.as_str())
            .fetch_one(&mut self.connection)
            .await?;
            if row.try_get::<Vec<u8>, _>("sha256")?
                != digest_bytes(e.sha256.0.as_ref().ok_or(ControlError::Conflict)?.as_str())
                || row.try_get::<i64, _>("byte_size")?
                    != e.byte_size
                        .0
                        .as_ref()
                        .ok_or(ControlError::Conflict)?
                        .value() as i64
            {
                return Err(ControlError::Conflict);
            }
            files.push(CheckpointFile {
                path: e.entry_key[5..].to_owned(),
                mode_bits: e.mode_bits.0.ok_or(ControlError::Conflict)? as u32,
                file,
            });
        }
        Ok(files)
    }
    async fn checkpoint_task(
        &mut self,
        w: &DomainId,
        a: &DomainId,
        c: &Closure,
        row: &RunRow,
    ) -> Result<(), ControlError> {
        let task = self.read_task_bundle(w, a, &row.task).await?;
        let candidate = BundleCandidate::inspect(&task.canonical_bytes, w)
            .map_err(|_| ControlError::Conflict)?;
        let resource = candidate
            .records()
            .iter()
            .find(|r| r.table == "tasks" && r.id == row.task)
            .and_then(|r| r.record.get("resource_id"))
            .and_then(|v| v.as_str())
            .ok_or(ControlError::Conflict)?;
        if task.receipt.version_id != row.revision
            || resource != c.manifest.owner_resource_id.as_str()
        {
            return Err(ControlError::Conflict);
        }
        Ok(())
    }
    /// Persists a verified journal cut. Does not create attempts or grant dispatch.
    pub async fn publish_checkpoint(
        &mut self,
        i: CheckpointPublication<'_>,
    ) -> Result<CheckpointReceipt, ControlError> {
        let identity = self.verified_identity(i.workspace, i.actor).await?;
        let c = decode_checkpoint(i.bytes, i.workspace)?;
        let bytes = c.candidate.canonical_bytes();
        if digest(bytes) != i.sha256 {
            return Err(ControlError::Conflict);
        }
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM local_checkpoints WHERE workspace_id=? AND id=?)",
        )
        .bind(i.workspace.as_str())
        .bind(c.checkpoint.id.as_str())
        .fetch_one(&mut self.connection)
        .await?;
        if exists {
            let saved = self
                .read_checkpoint(i.workspace, i.actor, &c.checkpoint.run_id, &c.checkpoint.id)
                .await?;
            if saved.canonical_bytes != bytes
                || saved.receipt.bundle_blob_id != *i.bundle_blob
                || saved.receipt.event_id != *i.event
                || saved.receipt.event_blob_id != *i.event_blob
            {
                return Err(ControlError::IdempotencyMismatch);
            }
            return Ok(saved.receipt);
        }
        let row = run_row(
            &mut self.connection,
            i.workspace,
            &c.checkpoint.run_id,
            i.actor,
        )
        .await?;
        require_local(&row, identity.device())?;
        cut(&row, &c, i.expected_version)?;
        self.checkpoint_task(i.workspace, i.actor, &c, &row).await?;
        attempt(&mut self.connection, i.workspace, &c, true).await?;
        self.checkpoint_files(i.workspace, i.actor, &c).await?;
        let created: i64 = sqlx::query_scalar(
            "SELECT created_at_us FROM local_runs WHERE workspace_id=? AND id=?",
        )
        .bind(i.workspace.as_str())
        .bind(c.checkpoint.run_id.as_str())
        .fetch_one(&mut self.connection)
        .await?;
        if c.checkpoint.created_at.unix_micros() < created {
            return Err(ControlError::Conflict);
        }
        let receipt = CheckpointReceipt {
            checkpoint_id: c.checkpoint.id.clone(),
            run_id: c.checkpoint.run_id.clone(),
            attempt_id: c.checkpoint.attempt_id.clone(),
            manifest_id: c.manifest.id.clone(),
            bundle_blob_id: i.bundle_blob.clone(),
            event_id: i.event.clone(),
            event_blob_id: i.event_blob.clone(),
            event_seq: positive(next(row.event)?)?,
            row_version: positive(next(row.version)?)?,
            bundle_digest: avencrew_contracts::scalars::Digest::new(hex(&i.sha256))
                .map_err(|_| ControlError::Conflict)?,
        };
        let payload = canonical(&receipt)?;
        self.put_canonical(i.workspace, i.bundle_blob, bytes)?;
        self.put_canonical(i.workspace, i.event_blob, &payload)?;
        let mut tx = self.connection.begin().await?;
        identity.check_live(&mut tx).await?;
        let current = run_row(&mut tx, i.workspace, &c.checkpoint.run_id, i.actor).await?;
        require_local(&current, identity.device())?;
        cut(&current, &c, i.expected_version)?;
        attempt(&mut tx, i.workspace, &c, true).await?;
        let at = c.checkpoint.created_at.unix_micros();
        blob(
            &mut tx,
            i.workspace,
            i.actor,
            i.bundle_blob,
            bytes,
            MEDIA,
            at,
        )
        .await?;
        blob(
            &mut tx,
            i.workspace,
            i.actor,
            i.event_blob,
            &payload,
            COMMIT_MEDIA,
            at,
        )
        .await?;
        sqlx::query("INSERT INTO local_manifests (id,workspace_id,created_at_us,owner_actor_id,kind,schema_version,canonical_blob_id,digest,sealed_at_us,deletion_generation) VALUES (?,?,?,?,'checkpoint',1,?,?,?,0)").bind(c.manifest.id.as_str()).bind(i.workspace.as_str()).bind(at).bind(i.actor.as_str()).bind(i.bundle_blob.as_str()).bind(digest_bytes(c.manifest.digest.as_str())).bind(c.manifest.sealed_at.unix_micros()).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO local_checkpoints (id,workspace_id,created_at_us,run_id,attempt_id,manifest_id,event_seq,accepted_command_seq,applied_command_seq,kind,schema_version,digest,sealed_at_us) VALUES (?,?,?,?,?,?,?,?,?,?,1,?,?)").bind(c.checkpoint.id.as_str()).bind(i.workspace.as_str()).bind(at).bind(c.checkpoint.run_id.as_str()).bind(c.checkpoint.attempt_id.as_str()).bind(c.manifest.id.as_str()).bind(current.event).bind(current.accepted).bind(current.applied).bind(&c.checkpoint.checkpoint_kind).bind(digest_bytes(c.checkpoint.digest.as_str())).bind(c.checkpoint.sealed_at.unix_micros()).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO local_events (id,workspace_id,created_at_us,run_id,attempt_id,seq,epoch,command_id,event_type,schema_version,occurred_at_us,payload_blob_id) VALUES (?,?,?,?,?,?,?,NULL,'checkpoint.committed',1,?,?)").bind(i.event.as_str()).bind(i.workspace.as_str()).bind(at).bind(c.checkpoint.run_id.as_str()).bind(c.checkpoint.attempt_id.as_str()).bind(receipt.event_seq.value() as i64).bind(current.epoch).bind(c.checkpoint.sealed_at.unix_micros()).bind(i.event_blob.as_str()).execute(&mut *tx).await?;
        let updated=sqlx::query("UPDATE local_runs SET current_checkpoint_id=?,last_event_seq=?,row_version=? WHERE workspace_id=? AND id=?").bind(c.checkpoint.id.as_str()).bind(receipt.event_seq.value() as i64).bind(receipt.row_version.value() as i64).bind(i.workspace.as_str()).bind(c.checkpoint.run_id.as_str()).execute(&mut *tx).await?;
        if updated.rows_affected() != 1 {
            return Err(ControlError::Conflict);
        }
        #[cfg(test)]
        tests::commit_barrier(&self.status.database_path, "before_commit")?;
        tx.commit().await?;
        #[cfg(test)]
        tests::commit_barrier(&self.status.database_path, "after_commit")?;
        Ok(receipt)
    }
    /// Returns verified captured files and explicit gaps, never an executable resume.
    pub async fn read_checkpoint(
        &mut self,
        w: &DomainId,
        a: &DomainId,
        run: &DomainId,
        id: &DomainId,
    ) -> Result<RestoredCheckpoint, ControlError> {
        let identity = self.verified_identity(w, a).await?;
        let current = run_row(&mut self.connection, w, run, a).await?;
        let cp=sqlx::query("SELECT c.*,m.canonical_blob_id,m.owner_actor_id,m.digest AS manifest_digest,m.created_at_us AS manifest_created,m.sealed_at_us AS manifest_sealed,m.kind AS manifest_kind,m.schema_version AS manifest_schema,m.deletion_generation FROM local_checkpoints c JOIN local_manifests m ON m.workspace_id=c.workspace_id AND m.id=c.manifest_id WHERE c.workspace_id=? AND c.id=? AND c.run_id=?").bind(w.as_str()).bind(id.as_str()).bind(run.as_str()).fetch_optional(&mut self.connection).await?.ok_or(ControlError::NotFoundOrDenied)?;
        let blob_id = DomainId::new(cp.try_get::<String, _>("canonical_blob_id")?)
            .map_err(|_| ControlError::Conflict)?;
        let bytes = self.read_registered_blob(w, a, &blob_id, MEDIA).await?;
        let c = decode_checkpoint(&bytes, w)?;
        if c.candidate.canonical_bytes() != bytes
            || c.checkpoint.id != *id
            || c.checkpoint.run_id != *run
            || cp.try_get::<String, _>("owner_actor_id")? != a.as_str()
            || cp.try_get::<String, _>("attempt_id")? != c.checkpoint.attempt_id.as_str()
            || cp.try_get::<String, _>("manifest_id")? != c.manifest.id.as_str()
            || cp.try_get::<String, _>("kind")? != c.checkpoint.checkpoint_kind
            || cp.try_get::<i64, _>("schema_version")? != 1
            || cp.try_get::<Vec<u8>, _>("digest")? != digest_bytes(c.checkpoint.digest.as_str())
            || cp.try_get::<Vec<u8>, _>("manifest_digest")?
                != digest_bytes(c.manifest.digest.as_str())
            || cp.try_get::<String, _>("manifest_kind")? != "checkpoint"
            || cp.try_get::<i64, _>("manifest_schema")? != 1
            || cp.try_get::<i64, _>("deletion_generation")? != 0
        {
            return Err(ControlError::Conflict);
        }
        for (name, expected) in [
            ("created_at_us", c.checkpoint.created_at.unix_micros()),
            ("sealed_at_us", c.checkpoint.sealed_at.unix_micros()),
            ("manifest_created", c.manifest.created_at.unix_micros()),
            ("manifest_sealed", c.manifest.sealed_at.unix_micros()),
            ("event_seq", c.checkpoint.event_seq.value() as i64),
            (
                "accepted_command_seq",
                c.checkpoint.accepted_command_seq.value() as i64,
            ),
            (
                "applied_command_seq",
                c.checkpoint.applied_command_seq.value() as i64,
            ),
        ] {
            if cp.try_get::<i64, _>(name)? != expected {
                return Err(ControlError::Conflict);
            }
        }
        if c.checkpoint.event_seq.value() as i64 >= current.event
            || c.checkpoint.accepted_command_seq.value() as i64 > current.accepted
            || c.checkpoint.applied_command_seq.value() as i64 > current.applied
            || c.coverage.attempt_epoch.value() as i64 > current.epoch
        {
            return Err(ControlError::Conflict);
        }
        self.checkpoint_task(w, a, &c, &current).await?;
        attempt(&mut self.connection, w, &c, false).await?;
        let event =
            sqlx::query("SELECT * FROM local_events WHERE workspace_id=? AND run_id=? AND seq=?")
                .bind(w.as_str())
                .bind(run.as_str())
                .bind(next(c.checkpoint.event_seq.value() as i64)?)
                .fetch_one(&mut self.connection)
                .await?;
        let event_blob = DomainId::new(event.try_get::<String, _>("payload_blob_id")?)
            .map_err(|_| ControlError::Conflict)?;
        let payload = self
            .read_registered_blob(w, a, &event_blob, COMMIT_MEDIA)
            .await?;
        let receipt: CheckpointReceipt =
            serde_json::from_slice(&payload).map_err(|_| ControlError::Conflict)?;
        if canonical(&receipt)? != payload
            || receipt.checkpoint_id != *id
            || receipt.run_id != *run
            || receipt.attempt_id != c.checkpoint.attempt_id
            || receipt.manifest_id != c.manifest.id
            || receipt.bundle_blob_id != blob_id
            || receipt.event_blob_id != event_blob
            || receipt.event_id.as_str() != event.try_get::<String, _>("id")?
            || receipt.event_seq.value() != c.checkpoint.event_seq.value() + 1
            || receipt.row_version.value() as i64 > current.version
            || receipt.bundle_digest.as_str() != hex(&digest(&bytes))
            || event.try_get::<String, _>("event_type")? != "checkpoint.committed"
            || event.try_get::<Option<String>, _>("command_id")?.is_some()
            || event.try_get::<Option<String>, _>("attempt_id")?.as_deref()
                != Some(c.checkpoint.attempt_id.as_str())
            || event.try_get::<i64, _>("epoch")? != c.coverage.attempt_epoch.value() as i64
            || event.try_get::<i64, _>("schema_version")? != 1
            || event.try_get::<i64, _>("created_at_us")? != c.checkpoint.created_at.unix_micros()
            || event.try_get::<i64, _>("occurred_at_us")? != c.checkpoint.sealed_at.unix_micros()
        {
            return Err(ControlError::Conflict);
        }
        let files = self.checkpoint_files(w, a, &c).await?;
        identity.check_live(&mut self.connection).await?;
        Ok(RestoredCheckpoint {
            receipt,
            canonical_bytes: bytes,
            coverage: c.coverage,
            files,
        })
    }
}

#[cfg(test)]
pub(crate) mod tests;
