//! Explicit private snapshots. Verification never confers restored authority.
use super::{
    blobs::{canonical, digest, hex},
    LocalStore, StoreError, ENGINE_VERSION, SCHEMA_VERSION,
};
use avencrew_contracts::{
    bundles::{RecordCandidate, RecordKey},
    canonical_bytes,
    scalars::{Counter, Digest, DomainId, Instant},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use sqlx::{sqlite::SqliteConnectOptions, Connection, Row, SqliteConnection};
use std::{
    collections::BTreeSet,
    fmt,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Component, Path, PathBuf},
};
const MAX_BLOBS: usize = 4096;
const MAX_JSON: u64 = 1024 * 1024;
const MAX_RAW: u64 = 64 * 1024 * 1024;
const MAX_DB: u64 = 1024 * 1024 * 1024;
const MAX_TOTAL: u64 = 2 * 1024 * 1024 * 1024;
const FORMAT: &str = "avencrew.local-backup/1";
#[derive(Debug)]
pub enum BackupError {
    Store(StoreError),
    InvalidPath,
    DestinationExists,
    LimitExceeded,
    InvalidManifest,
    SchemaMismatch,
    InventoryMismatch,
    UnavailableBlob,
}
impl fmt::Display for BackupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Store(_) => "backup storage failure",
            Self::InvalidPath => "invalid backup path",
            Self::DestinationExists => "backup destination exists",
            Self::LimitExceeded => "backup operation limit",
            Self::InvalidManifest => "invalid backup manifest",
            Self::SchemaMismatch => "backup schema mismatch",
            Self::InventoryMismatch => "backup inventory mismatch",
            Self::UnavailableBlob => "backup object unavailable",
        })
    }
}
impl std::error::Error for BackupError {}
impl From<StoreError> for BackupError {
    fn from(e: StoreError) -> Self {
        Self::Store(e)
    }
}
impl From<std::io::Error> for BackupError {
    fn from(e: std::io::Error) -> Self {
        Self::Store(e.into())
    }
}
impl From<sqlx::Error> for BackupError {
    fn from(e: sqlx::Error) -> Self {
        Self::Store(e.into())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupReceipt {
    pub schema_version: String,
    pub backup_id: DomainId,
    pub created_at: Instant,
    pub database_sha256: Digest,
    pub manifest_sha256: Digest,
    pub blob_count: Counter,
    pub total_bytes: Counter,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: String,
    backup_id: DomainId,
    created_at: Instant,
    sqlite_version: String,
    journal_schema_version: i64,
    database_sha256: Digest,
    database_bytes: Counter,
    blobs: Vec<Blob>,
}
#[derive(PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Blob {
    workspace_id: DomainId,
    blob_id: DomainId,
    sha256: Digest,
    byte_size: Counter,
    media_type: String,
    storage_relpath: String,
}
#[derive(Debug, Serialize)]
pub struct BackupInspection {
    pub receipt: BackupReceipt,
    pub journal_schema_version: i64,
    pub disposition: &'static str,
}
fn count(n: u64) -> Result<Counter, BackupError> {
    Counter::new(n.to_string()).map_err(|_| BackupError::LimitExceeded)
}
fn hash(bytes: &[u8]) -> Result<Digest, BackupError> {
    Digest::new(hex(&digest(bytes))).map_err(|_| BackupError::InvalidManifest)
}
// Walk names without canonicalizing away an untrusted symlink or '..'. The
// destination parent must already exist; creating a directory fleet is not needed.
fn absolute(path: &Path, last_missing: bool) -> Result<(), BackupError> {
    if !path.is_absolute() {
        return Err(BackupError::InvalidPath);
    }
    let mut p = PathBuf::new();
    let components: Vec<_> = path.components().collect();
    for (i, c) in components.iter().enumerate() {
        match c {
            Component::RootDir => p.push(c.as_os_str()),
            Component::Normal(_) => p.push(c.as_os_str()),
            _ => return Err(BackupError::InvalidPath),
        }
        match fs::symlink_metadata(&p) {
            Ok(m) if !m.file_type().is_symlink() && (i + 1 == components.len() || m.is_dir()) => (),
            Err(e)
                if e.kind() == std::io::ErrorKind::NotFound
                    && last_missing
                    && i + 1 == components.len() => {}
            _ => return Err(BackupError::InvalidPath),
        }
    }
    Ok(())
}
fn directory(path: &Path, uid: u32) -> Result<(), BackupError> {
    let m = fs::symlink_metadata(path)?;
    if !m.is_dir() || m.uid() != uid || m.mode() & 0o7777 != 0o700 {
        return Err(BackupError::InvalidPath);
    }
    Ok(())
}
fn open_file(root: &Path, relative: &str, uid: u32, limit: u64) -> Result<File, BackupError> {
    let rel = Path::new(relative);
    if rel.is_absolute() || rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(BackupError::InvalidPath);
    }
    directory(root, uid)?;
    let mut path = root.to_path_buf();
    let components: Vec<_> = rel.components().collect();
    for c in &components[..components.len().saturating_sub(1)] {
        path.push(c.as_os_str());
        directory(&path, uid)?;
    }
    path.push(
        components
            .last()
            .ok_or(BackupError::InvalidPath)?
            .as_os_str(),
    );
    let named = fs::symlink_metadata(&path)?;
    if !named.is_file()
        || named.nlink() != 1
        || named.uid() != uid
        || named.mode() & 0o7777 != 0o600
        || named.len() > limit
    {
        return Err(BackupError::UnavailableBlob);
    }
    let f = File::open(&path)?;
    let m = f.metadata()?;
    if m.ino() != named.ino() || m.dev() != named.dev() {
        return Err(BackupError::UnavailableBlob);
    }
    Ok(f)
}
fn stream(
    mut source: File,
    mut destination: Option<File>,
    expected_size: u64,
    expected_hash: Option<&Digest>,
) -> Result<Digest, BackupError> {
    let before = source.metadata()?;
    if before.len() != expected_size {
        return Err(BackupError::InventoryMismatch);
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0; 32768];
    let mut total = 0u64;
    loop {
        let n = source.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        total = total
            .checked_add(n as u64)
            .ok_or(BackupError::LimitExceeded)?;
        if total > expected_size {
            return Err(BackupError::InventoryMismatch);
        }
        hasher.update(&buffer[..n]);
        if let Some(f) = &mut destination {
            f.write_all(&buffer[..n])?;
        }
    }
    let after = source.metadata()?;
    let actual =
        Digest::new(hex(&hasher.finalize())).map_err(|_| BackupError::InventoryMismatch)?;
    if total != expected_size
        || before.len() != after.len()
        || before.mtime() != after.mtime()
        || before.mtime_nsec() != after.mtime_nsec()
        || before.ctime() != after.ctime()
        || before.ctime_nsec() != after.ctime_nsec()
        || expected_hash.is_some_and(|h| *h != actual)
    {
        return Err(BackupError::InventoryMismatch);
    }
    if let Some(f) = destination {
        f.sync_all()?;
    }
    Ok(actual)
}
fn new_file(root: &Path, relative: &str, uid: u32) -> Result<File, BackupError> {
    let mut path = root.to_path_buf();
    let rel = Path::new(relative);
    if rel.is_absolute() || rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(BackupError::InvalidPath);
    }
    for c in rel.parent().ok_or(BackupError::InvalidPath)?.components() {
        path.push(c.as_os_str());
        match fs::symlink_metadata(&path) {
            Ok(_) => directory(&path, uid)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                fs::DirBuilder::new().mode(0o700).create(&path)?;
                File::open(path.parent().ok_or(BackupError::InvalidPath)?)?.sync_all()?;
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(root.join(relative))?)
}
fn raw(b: &Blob) -> bool {
    b.media_type == "application/vnd.avencrew.vault-bytes"
}
fn internal_json(media: &str) -> bool {
    matches!(
        media,
        "application/vnd.avencrew.local-registration+json"
            | "application/vnd.avencrew.canonical-record+json"
            | "application/vnd.avencrew.record-bundle+json"
            | "application/vnd.avencrew.bundle-record+json"
            | "application/vnd.avencrew.local-run-registration+json"
            | "application/vnd.avencrew.control-command+json"
            | "application/vnd.avencrew.control-accepted+json"
            | "application/vnd.avencrew.checkpoint-bundle+json"
            | "application/vnd.avencrew.checkpoint-committed+json"
    )
}
fn locator(b: &Blob) -> Result<(), BackupError> {
    let prefix = format!("blobs/{}/", b.workspace_id.as_str());
    if b.media_type.starts_with("application/vnd.avencrew.")
        && !raw(b)
        && !internal_json(&b.media_type)
    {
        return Err(BackupError::UnavailableBlob);
    }
    if b.storage_relpath.contains(['\\', '\0'])
        || b.media_type.is_empty()
        || b.media_type.len() > 4096
        || b.media_type.chars().any(char::is_control)
    {
        return Err(BackupError::InventoryMismatch);
    }
    if raw(b) {
        if b.storage_relpath != format!("{prefix}vault/{}.bin", b.blob_id.as_str())
            || b.byte_size.value() > MAX_RAW
        {
            return Err(BackupError::InventoryMismatch);
        }
    } else {
        if b.byte_size.value() > MAX_JSON {
            return Err(BackupError::LimitExceeded);
        }
        if b.media_type == "application/vnd.avencrew.bundle-record+json" {
            let rest = b
                .storage_relpath
                .strip_prefix(&format!("{prefix}records/"))
                .ok_or(BackupError::InventoryMismatch)?;
            let (table, file) = rest.split_once('/').ok_or(BackupError::InventoryMismatch)?;
            if !matches!(
                table,
                "resources"
                    | "resource_versions"
                    | "checks"
                    | "tasks"
                    | "task_revisions"
                    | "task_checks"
                    | "artifacts"
                    | "artifact_versions"
                    | "blobs"
            ) || DomainId::new(
                file.strip_suffix(".json")
                    .ok_or(BackupError::InventoryMismatch)?,
            )
            .is_err()
            {
                return Err(BackupError::InventoryMismatch);
            }
        } else if b.storage_relpath != format!("{prefix}{}.json", b.blob_id.as_str()) {
            return Err(BackupError::InventoryMismatch);
        }
    }
    Ok(())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Index {
    schema_version: i32,
    root: RecordKey,
    bundle_blob_id: DomainId,
    record: RecordCandidate,
}
fn verify_json(root: &Path, b: &Blob, uid: u32) -> Result<(), BackupError> {
    if !internal_json(&b.media_type) {
        return Ok(());
    }
    let mut bytes = Vec::new();
    open_file(root, &b.storage_relpath, uid, MAX_JSON)?
        .take(MAX_JSON + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 != b.byte_size.value()
        || hash(&bytes)? != b.sha256
        || canonical_bytes(&bytes).map_err(|_| BackupError::InventoryMismatch)? != bytes
    {
        return Err(BackupError::InventoryMismatch);
    }
    if b.media_type == "application/vnd.avencrew.bundle-record+json" {
        let i: Index =
            serde_json::from_slice(&bytes).map_err(|_| BackupError::InventoryMismatch)?;
        if i.schema_version != 1
            || !matches!(i.root.table.as_str(), "tasks" | "artifacts")
            || i.record.record.get("workspace_id").and_then(|v| v.as_str())
                != Some(b.workspace_id.as_str())
            || i.record.record.get("id").and_then(|v| v.as_str()) != Some(i.record.id.as_str())
            || b.storage_relpath
                != format!(
                    "blobs/{}/records/{}/{}.json",
                    b.workspace_id.as_str(),
                    i.record.table,
                    i.record.id.as_str()
                )
        {
            return Err(BackupError::InventoryMismatch);
        }
        // Parent ID must be present as a canonical bundle, not merely named by an index.
        let parent = format!(
            "blobs/{}/{}.json",
            b.workspace_id.as_str(),
            i.bundle_blob_id.as_str()
        );
        let mut bytes = Vec::new();
        open_file(root, &parent, uid, MAX_JSON)?
            .take(MAX_JSON + 1)
            .read_to_end(&mut bytes)?;
        let candidate =
            avencrew_contracts::bundles::BundleCandidate::inspect(&bytes, &b.workspace_id)
                .map_err(|_| BackupError::InventoryMismatch)?;
        if candidate.root() != &i.root || !candidate.records().contains(&i.record) {
            return Err(BackupError::InventoryMismatch);
        }
    }
    Ok(())
}
async fn inventory(conn: &mut SqliteConnection) -> Result<Vec<Blob>, BackupError> {
    let rows=sqlx::query("SELECT b.*,a.status AS owner_status FROM local_blobs b JOIN local_actors a ON a.workspace_id=b.workspace_id AND a.id=b.owner_actor_id ORDER BY b.workspace_id,b.id LIMIT 4097").fetch_all(&mut *conn).await?;
    if rows.len() > MAX_BLOBS {
        return Err(BackupError::LimitExceeded);
    }
    let fenced:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM local_erasure_fences) OR EXISTS(SELECT 1 FROM local_workspaces WHERE status<>'active' OR permission_generation<>0 OR deletion_generation<>0)").fetch_one(&mut *conn).await?;
    if fenced {
        return Err(BackupError::UnavailableBlob);
    }
    let mut paths = BTreeSet::new();
    let mut result = Vec::new();
    for r in rows {
        if r.try_get::<String, _>("status")? != "verified"
            || r.try_get::<Option<i64>, _>("erased_at_us")?.is_some()
            || r.try_get::<String, _>("owner_status")? != "active"
        {
            return Err(BackupError::UnavailableBlob);
        }
        let b = Blob {
            workspace_id: DomainId::new(r.try_get::<String, _>("workspace_id")?)
                .map_err(|_| BackupError::InventoryMismatch)?,
            blob_id: DomainId::new(r.try_get::<String, _>("id")?)
                .map_err(|_| BackupError::InventoryMismatch)?,
            sha256: Digest::new(hex(&r.try_get::<Vec<u8>, _>("sha256")?))
                .map_err(|_| BackupError::InventoryMismatch)?,
            byte_size: count(
                u64::try_from(r.try_get::<i64, _>("byte_size")?)
                    .map_err(|_| BackupError::InventoryMismatch)?,
            )?,
            media_type: r.try_get("media_type")?,
            storage_relpath: r.try_get("storage_relpath")?,
        };
        locator(&b)?;
        if !paths.insert(b.storage_relpath.clone()) {
            return Err(BackupError::InventoryMismatch);
        }
        result.push(b);
    }
    Ok(result)
}
async fn validate_database(conn: &mut SqliteConnection) -> Result<(), BackupError> {
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&mut *conn)
        .await?;
    if version != SCHEMA_VERSION {
        return Err(BackupError::SchemaMismatch);
    }
    super::verify_schema(conn, version).await?;
    let engine: String = sqlx::query_scalar("SELECT sqlite_version()")
        .fetch_one(&mut *conn)
        .await?;
    let source: String = sqlx::query_scalar("SELECT sqlite_source_id()")
        .fetch_one(&mut *conn)
        .await?;
    if engine != ENGINE_VERSION || source != super::ENGINE_SOURCE {
        return Err(BackupError::SchemaMismatch);
    }
    let actual: Vec<(i64, String, bool, Vec<u8>)> = sqlx::query_as(
        "SELECT version,description,success,checksum FROM _sqlx_migrations ORDER BY version",
    )
    .fetch_all(&mut *conn)
    .await?;
    let expected: Vec<_> = super::migrator()
        .iter()
        .map(|m| {
            (
                m.version,
                m.description.to_string(),
                true,
                m.checksum.to_vec(),
            )
        })
        .collect();
    let check: String = sqlx::query_scalar("PRAGMA quick_check")
        .fetch_one(&mut *conn)
        .await?;
    if actual != expected
        || check != "ok"
        || !sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(conn)
            .await?
            .is_empty()
    {
        return Err(BackupError::SchemaMismatch);
    }
    Ok(())
}
fn total(m: &Manifest, manifest_bytes: u64) -> Result<u64, BackupError> {
    let mut n = m
        .database_bytes
        .value()
        .checked_add(manifest_bytes)
        .ok_or(BackupError::LimitExceeded)?;
    for b in &m.blobs {
        n = n
            .checked_add(b.byte_size.value())
            .ok_or(BackupError::LimitExceeded)?;
    }
    if n > MAX_TOTAL {
        return Err(BackupError::LimitExceeded);
    }
    Ok(n)
}
fn receipt(m: &Manifest, bytes: &[u8]) -> Result<BackupReceipt, BackupError> {
    Ok(BackupReceipt {
        schema_version: FORMAT.into(),
        backup_id: m.backup_id.clone(),
        created_at: m.created_at.clone(),
        database_sha256: m.database_sha256.clone(),
        manifest_sha256: hash(bytes)?,
        blob_count: count(m.blobs.len() as u64)?,
        total_bytes: count(total(m, bytes.len() as u64)?)?,
    })
}
fn members(root: &Path, m: &Manifest, uid: u32) -> Result<(), BackupError> {
    let mut files: BTreeSet<PathBuf> = [
        PathBuf::from("execution.sqlite3"),
        PathBuf::from("manifest.json"),
    ]
    .into();
    let mut dirs = BTreeSet::new();
    for b in &m.blobs {
        let p = PathBuf::from(&b.storage_relpath);
        for a in p.ancestors().skip(1).filter(|a| !a.as_os_str().is_empty()) {
            dirs.insert(a.to_path_buf());
        }
        files.insert(p);
    }
    let mut pending = vec![PathBuf::new()];
    let mut seen = 0usize;
    while let Some(folder) = pending.pop() {
        for entry in fs::read_dir(root.join(&folder))? {
            seen += 1;
            if seen > 32768 {
                return Err(BackupError::LimitExceeded);
            }
            let e = entry?;
            let rel = folder.join(e.file_name());
            let kind = e.file_type()?;
            if kind.is_dir() && dirs.contains(&rel) {
                directory(&e.path(), uid)?;
                pending.push(rel);
            } else if kind.is_file() && files.remove(&rel) {
                let limit = if rel == Path::new("execution.sqlite3") {
                    MAX_DB
                } else {
                    MAX_RAW
                };
                open_file(
                    root,
                    rel.to_str().ok_or(BackupError::InvalidPath)?,
                    uid,
                    limit,
                )?;
            } else {
                return Err(BackupError::InventoryMismatch);
            }
        }
    }
    if !files.is_empty() {
        return Err(BackupError::InventoryMismatch);
    }
    Ok(())
}
impl LocalStore {
    /// Offline explicit operation. Source ownership remains held through publication.
    pub async fn create_backup(
        &mut self,
        destination: &Path,
        backup_id: &DomainId,
        at: &Instant,
    ) -> Result<BackupReceipt, BackupError> {
        absolute(destination, true)?;
        let source = self
            .status
            .database_path
            .parent()
            .ok_or(BackupError::InvalidPath)?
            .to_path_buf();
        if destination.starts_with(&source) || source.starts_with(destination) {
            return Err(BackupError::InvalidPath);
        }
        if fs::symlink_metadata(destination).is_ok() {
            return Err(BackupError::DestinationExists);
        }
        super::verify_settings(&mut self.connection).await?;
        validate_database(&mut self.connection).await?;
        let blobs = inventory(&mut self.connection).await?;
        let estimated: u64 = blobs.iter().map(|b| b.byte_size.value()).sum();
        if estimated > MAX_TOTAL {
            return Err(BackupError::LimitExceeded);
        }
        fs::DirBuilder::new().mode(0o700).create(destination)?;
        let uid = fs::symlink_metadata(&source)?.uid();
        directory(destination, uid)?;
        File::open(destination.parent().ok_or(BackupError::InvalidPath)?)?.sync_all()?;
        // Establish the durable non-live marker BEFORE any retained database
        // bytes can appear here. Keep this file until final manifest publication.
        let mut pending_manifest = new_file(destination, "manifest.pending", uid)?;
        pending_manifest.sync_all()?;
        File::open(destination)?.sync_all()?;
        let snapshot = new_file(destination, "execution.sqlite3", uid)?;
        sqlx::query("VACUUM INTO ?")
            .bind(
                destination
                    .join("execution.sqlite3")
                    .to_str()
                    .ok_or(BackupError::InvalidPath)?,
            )
            .execute(&mut self.connection)
            .await?;
        snapshot.sync_all()?;
        drop(snapshot);
        let mut conn = SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(destination.join("execution.sqlite3"))
                .read_only(true)
                .immutable(true),
        )
        .await?;
        let checked = async {
            validate_database(&mut conn).await?;
            if inventory(&mut conn).await? != blobs {
                return Err(BackupError::InventoryMismatch);
            }
            Ok::<_, BackupError>(())
        }
        .await;
        conn.close().await?;
        checked?;
        #[cfg(test)]
        tests::publication_barrier(destination, "after_snapshot")?;
        let database_size = fs::metadata(destination.join("execution.sqlite3"))?.len();
        let database_hash = stream(
            open_file(destination, "execution.sqlite3", uid, MAX_DB)?,
            None,
            database_size,
            None,
        )?;
        if database_size
            .checked_add(estimated)
            .and_then(|n| n.checked_add(MAX_JSON))
            .is_none_or(|n| n > MAX_TOTAL)
        {
            return Err(BackupError::LimitExceeded);
        }
        // Copy all objects first, then validate indexes against their copied parents.
        for b in &blobs {
            stream(
                open_file(
                    &source,
                    &b.storage_relpath,
                    uid,
                    if raw(b) { MAX_RAW } else { MAX_JSON },
                )?,
                Some(new_file(destination, &b.storage_relpath, uid)?),
                b.byte_size.value(),
                Some(&b.sha256),
            )?;
            File::open(
                destination
                    .join(&b.storage_relpath)
                    .parent()
                    .ok_or(BackupError::InvalidPath)?,
            )?
            .sync_all()?;
        }
        for b in &blobs {
            stream(
                open_file(
                    destination,
                    &b.storage_relpath,
                    uid,
                    if raw(b) { MAX_RAW } else { MAX_JSON },
                )?,
                None,
                b.byte_size.value(),
                Some(&b.sha256),
            )?;
            verify_json(destination, b, uid)?;
        }
        if inventory(&mut self.connection).await? != blobs {
            return Err(BackupError::InventoryMismatch);
        }
        let manifest = Manifest {
            schema_version: FORMAT.into(),
            backup_id: backup_id.clone(),
            created_at: at.clone(),
            sqlite_version: ENGINE_VERSION.into(),
            journal_schema_version: SCHEMA_VERSION,
            database_sha256: database_hash,
            database_bytes: count(database_size)?,
            blobs,
        };
        let bytes = canonical(&manifest)?;
        let result = receipt(&manifest, &bytes)?;
        pending_manifest.write_all(&bytes)?;
        pending_manifest.sync_all()?;
        drop(pending_manifest);
        #[cfg(test)]
        tests::publication_barrier(destination, "before_publish")?;
        fs::hard_link(
            destination.join("manifest.pending"),
            destination.join("manifest.json"),
        )?;
        fs::remove_file(destination.join("manifest.pending"))?;
        File::open(destination)?.sync_all()?;
        #[cfg(test)]
        tests::publication_barrier(destination, "after_publish")?;
        Ok(result)
    }
    /// Read-only static inspection; never migrates, repairs or grants execution.
    pub async fn inspect_backup(source: &Path) -> Result<BackupInspection, BackupError> {
        absolute(source, false)?;
        let uid = fs::symlink_metadata(source)?.uid();
        directory(source, uid)?;
        let mut bytes = Vec::new();
        open_file(source, "manifest.json", uid, MAX_JSON)?
            .take(MAX_JSON + 1)
            .read_to_end(&mut bytes)?;
        let m: Manifest =
            serde_json::from_slice(&bytes).map_err(|_| BackupError::InvalidManifest)?;
        if canonical_bytes(&bytes).map_err(|_| BackupError::InvalidManifest)? != bytes
            || m.schema_version != FORMAT
            || m.sqlite_version != ENGINE_VERSION
            || m.journal_schema_version != SCHEMA_VERSION
            || m.database_bytes.value() > MAX_DB
            || m.blobs.len() > MAX_BLOBS
        {
            return Err(BackupError::InvalidManifest);
        }
        let mut previous = None;
        let mut paths = BTreeSet::new();
        for b in &m.blobs {
            locator(b)?;
            let key = (b.workspace_id.clone(), b.blob_id.clone());
            if previous.as_ref().is_some_and(|p| *p >= key)
                || !paths.insert(b.storage_relpath.clone())
            {
                return Err(BackupError::InvalidManifest);
            }
            previous = Some(key);
        }
        let result = receipt(&m, &bytes)?;
        members(source, &m, uid)?;
        stream(
            open_file(source, "execution.sqlite3", uid, MAX_DB)?,
            None,
            m.database_bytes.value(),
            Some(&m.database_sha256),
        )?;
        let mut conn = SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(source.join("execution.sqlite3"))
                .read_only(true)
                .immutable(true),
        )
        .await?;
        let checked = async {
            validate_database(&mut conn).await?;
            if inventory(&mut conn).await? != m.blobs {
                return Err(BackupError::InventoryMismatch);
            }
            Ok::<_, BackupError>(())
        }
        .await;
        conn.close().await?;
        checked?;
        for b in &m.blobs {
            stream(
                open_file(
                    source,
                    &b.storage_relpath,
                    uid,
                    if raw(b) { MAX_RAW } else { MAX_JSON },
                )?,
                None,
                b.byte_size.value(),
                Some(&b.sha256),
            )?;
            verify_json(source, b, uid)?;
        }
        Ok(BackupInspection {
            receipt: result,
            journal_schema_version: SCHEMA_VERSION,
            disposition: "verified_private_backup",
        })
    }
}
#[cfg(test)]
mod tests;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestoreReceipt {
    pub backup_id: DomainId,
    pub manifest_sha256: Digest,
    pub journal_schema_version: i64,
    pub disposition: String,
    pub reasons: Vec<String>,
}
impl LocalStore {
    /// Verified isolated copy, never a writable/dispatch-capable LocalStore.
    pub async fn restore_backup_quarantined(
        source: &Path,
        destination: &Path,
    ) -> Result<RestoreReceipt, BackupError> {
        let inspection = Self::inspect_backup(source).await?;
        absolute(destination, true)?;
        if destination.starts_with(source) || source.starts_with(destination) {
            return Err(BackupError::InvalidPath);
        }
        if fs::symlink_metadata(destination).is_ok() {
            return Err(BackupError::DestinationExists);
        }
        let uid = fs::symlink_metadata(source)?.uid();
        let mut bytes = Vec::new();
        open_file(source, "manifest.json", uid, MAX_JSON)?
            .take(MAX_JSON + 1)
            .read_to_end(&mut bytes)?;
        if hash(&bytes)? != inspection.receipt.manifest_sha256 {
            return Err(BackupError::InventoryMismatch);
        }
        let m: Manifest =
            serde_json::from_slice(&bytes).map_err(|_| BackupError::InvalidManifest)?;
        fs::DirBuilder::new().mode(0o700).create(destination)?;
        directory(destination, uid)?;
        let marker = canonical(
            &serde_json::json!({"schema_version":"avencrew.local-restore-quarantine/1","backup_id":m.backup_id,"manifest_sha256":inspection.receipt.manifest_sha256}),
        )?;
        let mut f = new_file(destination, "restore-quarantine.json", uid)?;
        f.write_all(&marker)?;
        f.sync_all()?;
        drop(f);
        File::open(destination)?.sync_all()?;
        File::open(destination.parent().ok_or(BackupError::InvalidPath)?)?.sync_all()?;
        #[cfg(test)]
        tests::publication_barrier(destination, "restore_marker")?;
        stream(
            open_file(source, "execution.sqlite3", uid, MAX_DB)?,
            Some(new_file(destination, "execution.sqlite3", uid)?),
            m.database_bytes.value(),
            Some(&m.database_sha256),
        )?;
        for b in &m.blobs {
            stream(
                open_file(
                    source,
                    &b.storage_relpath,
                    uid,
                    if raw(b) { MAX_RAW } else { MAX_JSON },
                )?,
                Some(new_file(destination, &b.storage_relpath, uid)?),
                b.byte_size.value(),
                Some(&b.sha256),
            )?;
            File::open(
                destination
                    .join(&b.storage_relpath)
                    .parent()
                    .ok_or(BackupError::InvalidPath)?,
            )?
            .sync_all()?;
        }
        // Verification is direct read-only SQLite, not an ordinary boot bypass.
        stream(
            open_file(destination, "execution.sqlite3", uid, MAX_DB)?,
            None,
            m.database_bytes.value(),
            Some(&m.database_sha256),
        )?;
        let mut conn = SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(destination.join("execution.sqlite3"))
                .read_only(true)
                .immutable(true),
        )
        .await?;
        let checked = async {
            validate_database(&mut conn).await?;
            if inventory(&mut conn).await? != m.blobs {
                return Err(BackupError::InventoryMismatch);
            }
            Ok::<_, BackupError>(())
        }
        .await;
        conn.close().await?;
        checked?;
        for b in &m.blobs {
            stream(
                open_file(
                    destination,
                    &b.storage_relpath,
                    uid,
                    if raw(b) { MAX_RAW } else { MAX_JSON },
                )?,
                None,
                b.byte_size.value(),
                Some(&b.sha256),
            )?;
            verify_json(destination, b, uid)?;
        }
        let result = RestoreReceipt {
            backup_id: m.backup_id,
            manifest_sha256: inspection.receipt.manifest_sha256,
            journal_schema_version: SCHEMA_VERSION,
            disposition: "quarantined".into(),
            reasons: vec![
                "authority_currency_unverified".into(),
                "deletion_replay_required".into(),
                "execution_reconciliation_required".into(),
            ],
        };
        let bytes = canonical(&result)?;
        let mut f = new_file(destination, "restore-inspection.pending", uid)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
        drop(f);
        #[cfg(test)]
        tests::publication_barrier(destination, "before_restore_publish")?;
        fs::hard_link(
            destination.join("restore-inspection.pending"),
            destination.join("restore-inspection.json"),
        )?;
        fs::remove_file(destination.join("restore-inspection.pending"))?;
        File::open(destination)?.sync_all()?;
        #[cfg(test)]
        tests::publication_barrier(destination, "after_restore_publish")?;
        Ok(result)
    }
}
