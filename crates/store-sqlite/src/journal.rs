//! Supervisor-owned file boot. No tool, renderer, pool or arbitrary SQL surface.
use std::borrow::Cow;
use std::fmt;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

use sqlx::migrate::{MigrateError, Migration, MigrationType, Migrator};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous};
use sqlx::{Connection, SqlSafeStr, SqliteConnection};

const SCHEMA_VERSION: i64 = 2;
mod backup;
mod blobs;
pub use backup::{BackupError, BackupInspection, BackupReceipt, RestoreReceipt};
mod commands;
mod diagnostics;
mod identity;
pub use diagnostics::{DiagnosticCounts, LocalDiagnostics};
mod publication;
mod vault;
pub use commands::{
    CheckpointFile, CheckpointPublication, CheckpointReceipt, CommandAcceptance, ControlError,
    FileCoverage, LocalRunReceipt, LocalRunRegistration, RecoveredControl, RecoveredRun,
    RecoveryGate, RecoverySnapshot, RestoredCheckpoint,
};
pub use identity::RegistrationReceipt;
pub use publication::{
    BundlePublication, Payload, PublicationReceipt, PublishedBundle, RecordAllocation,
};
pub use vault::{BlobImport, StagingCleanup, VaultReceipt};
const ENGINE_VERSION: &str = "3.53.4";
const ENGINE_SOURCE: &str =
    "2026-07-24 19:02:57 bf7c7f30031888f4e796e429ab3978879485813aaca6f641c7b33e4e09459bcc";
const MIGRATIONS: [(i64, &str, &str); 2] = [
    (
        1,
        "L0 namespace",
        include_str!("../../../migrations/sqlite/0001_l0_journal.sql"),
    ),
    (
        2,
        "L1 journal",
        include_str!("../../../migrations/sqlite/0002_l1_journal.sql"),
    ),
];

#[derive(Debug)]
pub enum StoreError {
    Io(io::Error),
    Database(sqlx::Error),
    Migration(MigrateError),
    WriterOwned,
    Incompatible(&'static str),
}
impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "journal filesystem: {e}"),
            Self::Database(e) => write!(f, "journal database: {e}"),
            Self::Migration(e) => write!(f, "journal migration: {e}"),
            Self::WriterOwned => f.write_str("journal already owned by another supervisor"),
            Self::Incompatible(reason) => write!(f, "journal refuses startup: {reason}"),
        }
    }
}
impl std::error::Error for StoreError {}
impl From<io::Error> for StoreError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<sqlx::Error> for StoreError {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e)
    }
}
impl From<MigrateError> for StoreError {
    fn from(e: MigrateError) -> Self {
        Self::Migration(e)
    }
}

/// Successful boot report, produced only after committed migrations and checks.
#[derive(Debug, Clone)]
pub struct StoreStatus {
    pub database_path: PathBuf,
    pub schema_version: i64,
    pub domain_tables: usize,
}

/// One connection and one OS lock, held for the store's entire lifetime.
/// Call `close` before releasing ownership; no public raw connection is exposed.
pub struct LocalStore {
    connection: SqliteConnection,
    _ownership: File,
    status: StoreStatus,
}

impl LocalStore {
    pub async fn open(root: &Path) -> Result<Self, StoreError> {
        Self::open_to(root, SCHEMA_VERSION).await
    }

    async fn open_to(root: &Path, target: i64) -> Result<Self, StoreError> {
        Self::open_mode(root, target, true).await
    }

    /// Local metadata export cannot create a database or migrate an old one.
    pub async fn open_existing_for_diagnostics(root: &Path) -> Result<Self, StoreError> {
        Self::open_mode(root, SCHEMA_VERSION, false).await
    }

    async fn open_mode(
        root: &Path,
        target: i64,
        allow_migration: bool,
    ) -> Result<Self, StoreError> {
        if !allow_migration {
            let metadata = fs::symlink_metadata(root)?;
            if !metadata.is_dir() {
                return Err(StoreError::Incompatible(
                    "diagnostics requires existing root",
                ));
            }
            validate_file(&root.join("execution.sqlite3"))?;
        }
        // Presence blocks boot even if malformed or a dangling symlink. No
        // restored state may enter ordinary boot before authority/deletion replay.
        for marker in [
            "restore-quarantine.json",
            "manifest.json",
            "manifest.pending",
        ] {
            match fs::symlink_metadata(root.join(marker)) {
                Ok(_) => {
                    return Err(StoreError::Incompatible(
                        "backup or restored store requires quarantine inspection",
                    ))
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => (),
                Err(e) => return Err(e.into()),
            }
        }
        let root = private_root(root)?;
        let ownership = private_file(&root.join("supervisor.lock"))?;
        match ownership.try_lock() {
            Ok(()) => (),
            Err(TryLockError::WouldBlock) => return Err(StoreError::WriterOwned),
            Err(TryLockError::Error(e)) => return Err(e.into()),
        }
        let database = root.join("execution.sqlite3");
        let file = private_file(&database)?;
        file.sync_all()?;
        drop(file);
        for suffix in ["-wal", "-shm", "-journal"] {
            validate_optional_file(&root.join(format!("execution.sqlite3{suffix}")))?;
        }
        let options = SqliteConnectOptions::new()
            .filename(&database)
            .create_if_missing(false)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Full)
            .busy_timeout(Duration::from_millis(5000));
        let mut connection = SqliteConnection::connect_with(&options).await?;
        // A rejected boot must await the SQLite worker's close while the
        // writer lock remains held; Drop only schedules asynchronous cleanup.
        let opened: Result<StoreStatus, StoreError> = async {
        verify_settings(&mut connection).await?;
        let version: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&mut connection)
            .await?;
        if version > target || version < 0 {
            return Err(StoreError::Incompatible(
                "unsupported newer schema; no downgrade",
            ));
        }
        if !allow_migration && version != target {
            return Err(StoreError::Incompatible(
                "diagnostics requires current schema",
            ));
        }
        let table_count: i64 = sqlx::query_scalar("SELECT count(*) FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name<>'_sqlx_migrations'")
            .fetch_one(&mut connection).await?;
        if version == 0 && table_count != 0 {
            return Err(StoreError::Incompatible("unversioned nonempty database"));
        }
        // SQLx's SQLite Migrate::lock is a no-op. The OS ownership lock above
        // serializes our supervisors; each SQLx migration commits DDL, ledger
        // and user_version in one transaction. Foreign writes remain OS-scoped.
        if version > 0 {
            verify_schema(&mut connection, version).await?;
            if version < target {
                snapshot_before_upgrade(&mut connection, &root, version).await?;
            }
        }
        let migrator = migrator();
        migrator.run_to(target, &mut connection).await?;
        let current: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&mut connection)
            .await?;
        if current != target {
            return Err(StoreError::Incompatible(
                "schema and migration ledger disagree",
            ));
        }
        verify_schema(&mut connection, current).await?;
        verify_settings(&mut connection).await?;
        let check: String = sqlx::query_scalar("PRAGMA quick_check")
            .fetch_one(&mut connection)
            .await?;
        if check != "ok" {
            return Err(StoreError::Incompatible("database quick_check failed"));
        }
        if !sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&mut connection)
            .await?
            .is_empty()
        {
            return Err(StoreError::Incompatible("existing foreign-key violation"));
        }
        for suffix in ["-wal", "-shm", "-journal"] {
            validate_optional_file(&root.join(format!("execution.sqlite3{suffix}")))?;
        }
        File::open(&root)?.sync_all()?;
        Ok(StoreStatus {
            database_path: database,
            schema_version: current,
            domain_tables: if current == 1 { 6 } else { 19 },
        })
        }.await;
        let status = match opened {
            Ok(status) => status,
            Err(error) => {
                connection.close().await?;
                ownership.unlock()?;
                return Err(error);
            }
        };
        Ok(Self {
            connection,
            _ownership: ownership,
            status,
        })
    }

    pub fn status(&self) -> &StoreStatus {
        &self.status
    }

    pub async fn close(self) -> Result<(), StoreError> {
        // Keep the lock until SQLx has finished closing the actual connection.
        self.connection.close().await?;
        // Explicitly release the lock: a concurrent fork can temporarily inherit
        // the descriptor before exec closes it, delaying release by drop alone.
        self._ownership.unlock()?;
        drop(self._ownership);
        Ok(())
    }
}

fn migrator() -> Migrator {
    Migrator {
        migrations: Cow::Owned(
            MIGRATIONS
                .iter()
                .map(|(version, description, sql)| {
                    Migration::new(
                        *version,
                        (*description).into(),
                        MigrationType::Simple,
                        (*sql).into_sql_str(),
                        false,
                    )
                })
                .collect(),
        ),
        ..Migrator::DEFAULT
    }
}

fn private_root(path: &Path) -> Result<PathBuf, StoreError> {
    if !path.is_absolute() {
        return Err(StoreError::Incompatible("data root must be absolute"));
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() && metadata.mode() & 0o777 == 0o700 => (),
        Ok(_) => {
            return Err(StoreError::Incompatible(
                "data root must be a real directory with mode 0700",
            ))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(path)?;
        }
        Err(e) => return Err(e.into()),
    }
    Ok(path.canonicalize()?)
}
fn private_file(path: &Path) -> Result<File, StoreError> {
    let file = match OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
    {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            validate_file(path)?;
            OpenOptions::new().read(true).write(true).open(path)?
        }
        Err(e) => return Err(e.into()),
    };
    let meta = file.metadata()?;
    let named = fs::symlink_metadata(path)?;
    if meta.ino() != named.ino() || meta.dev() != named.dev() {
        return Err(StoreError::Incompatible(
            "file identity changed during open",
        ));
    }
    validate_file(path)?;
    Ok(file)
}
fn validate_file(path: &Path) -> Result<(), StoreError> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.nlink() != 1 || meta.mode() & 0o777 != 0o600 {
        return Err(StoreError::Incompatible(
            "runtime files must be regular mode 0600 files without hard links",
        ));
    }
    Ok(())
}
fn validate_optional_file(path: &Path) -> Result<(), StoreError> {
    match fs::symlink_metadata(path) {
        Ok(_) => validate_file(path),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
async fn verify_settings(conn: &mut SqliteConnection) -> Result<(), StoreError> {
    let engine: String = sqlx::query_scalar("SELECT sqlite_version()")
        .fetch_one(&mut *conn)
        .await?;
    let source: String = sqlx::query_scalar("SELECT sqlite_source_id()")
        .fetch_one(&mut *conn)
        .await?;
    let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(&mut *conn)
        .await?;
    let sync: i64 = sqlx::query_scalar("PRAGMA synchronous")
        .fetch_one(&mut *conn)
        .await?;
    let fk: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(&mut *conn)
        .await?;
    let timeout: i64 = sqlx::query_scalar("PRAGMA busy_timeout")
        .fetch_one(&mut *conn)
        .await?;
    if engine != ENGINE_VERSION
        || source != ENGINE_SOURCE
        || mode != "wal"
        || sync != 2
        || fk != 1
        || timeout != 5000
    {
        return Err(StoreError::Incompatible(
            "engine or WAL/FULL/FK/timeout settings differ",
        ));
    }
    Ok(())
}
async fn verify_schema(conn: &mut SqliteConnection, version: i64) -> Result<(), StoreError> {
    // Ask the same pinned engine to build the expected schema in memory.
    // This verifies tables, CHECKs, FKs, indexes, triggers and SQLx's own ledger
    // without a handwritten SQL parser or duplicated schema definitions.
    let mut reference = SqliteConnection::connect("sqlite::memory:").await?;
    migrator().run_to(version, &mut reference).await?;
    const OBJECTS: &str = "SELECT type,name,sql FROM sqlite_schema WHERE substr(name,1,7)<>'sqlite_' ORDER BY type,name";
    let expected: Vec<(String, String, String)> =
        sqlx::query_as(OBJECTS).fetch_all(&mut reference).await?;
    reference.close().await?;
    let actual: Vec<(String, String, String)> = sqlx::query_as(OBJECTS).fetch_all(conn).await?;
    if actual != expected {
        return Err(StoreError::Incompatible(
            "schema objects differ from this release",
        ));
    }
    Ok(())
}
async fn snapshot_before_upgrade(
    conn: &mut SqliteConnection,
    root: &Path,
    version: i64,
) -> Result<(), StoreError> {
    let backups = private_root(&root.join("backups"))?;
    // Reserve a private new destination; never overwrite any previous snapshot.
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| StoreError::Incompatible("system clock before epoch"))?
        .as_nanos();
    let path = backups.join(format!("before-v{version}-{id}.sqlite3"));
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;
    // SQLite's supported SQL snapshot API includes committed WAL state. This
    // is never a main-file copy; output is checked and synced before migration.
    sqlx::query("VACUUM INTO ?")
        .bind(
            path.to_str()
                .ok_or(StoreError::Incompatible("non-UTF8 backup path"))?,
        )
        .execute(&mut *conn)
        .await?;
    file.sync_all()?;
    let mut snapshot = SqliteConnection::connect_with(
        &SqliteConnectOptions::new().filename(&path).read_only(true),
    )
    .await?;
    let check: String = sqlx::query_scalar("PRAGMA quick_check")
        .fetch_one(&mut snapshot)
        .await?;
    let prior: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&mut snapshot)
        .await?;
    snapshot.close().await?;
    if check != "ok" || prior != version {
        return Err(StoreError::Incompatible(
            "upgrade snapshot verification failed",
        ));
    }
    validate_file(&path)?;
    File::open(backups)?.sync_all()?;
    File::open(root)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests;
