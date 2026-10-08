use super::*;
use crate::journal::{
    commands::checkpoints::tests::{fixture, publish, ready},
    tests::{run, Root},
};
use std::{
    os::unix::fs::{symlink, PermissionsExt},
    process::{Command, Stdio},
    time::{Duration, Instant as Clock},
};
fn id(n: u32) -> DomainId {
    DomainId::new(format!("01900000-0000-7000-8000-{n:012x}")).unwrap()
}
fn at() -> Instant {
    Instant::new("2026-10-08T01:00:00.000000Z").unwrap()
}
async fn source(root: &Root) -> LocalStore {
    let mut s = ready(root).await;
    publish(&mut s, &fixture()).await.unwrap();
    let bytes=serde_json::to_vec(&serde_json::json!({"kind":"steer","command_id":id(600),"run_id":id(300),"idempotency_key":"backup-steer","payload":{"text":"Preserve accepted steering in backup","expected_revision":"2"}})).unwrap();
    s.accept_command(crate::CommandAcceptance {
        workspace: &id(1),
        actor: &id(2),
        bytes: &bytes,
        event: &id(601),
        queue: &id(602),
        command_blob: &id(603),
        event_blob: &id(604),
        at: &at(),
    })
    .await
    .unwrap();
    s
}
#[test]
fn complete_inventory_snapshot_preserves_checkpoint_and_source() {
    run(async {
        let root = Root::new();
        let parent = Root::new();
        let package = parent.0.join("snapshot");
        let mut s = source(&root).await;
        // Opaque publication payload is not JSON despite its allocated .json name.
        let v = crate::journal::publication::tests::fixture(
            avencrew_contracts::bundles::Purpose::Artifact,
            true,
        );
        let bytes = canonical(&v).unwrap();
        let records = crate::journal::publication::tests::allocations(&v);
        s.publish_bundle(crate::BundlePublication {
            workspace: &id(1),
            actor: &id(2),
            bundle_blob_id: &id(901),
            bytes: &bytes,
            sha256: digest(&bytes),
            record_blobs: &records,
            payloads: &[crate::Payload {
                id: &id(206),
                bytes: b"payload",
            }],
        })
        .await
        .unwrap();
        assert!(
            fs::metadata(root.0.join("execution.sqlite3-wal"))
                .unwrap()
                .len()
                > 0
        );
        let large = vec![0x5a; 2 * 1024 * 1024 + 11];
        s.import_vault_blob(
            crate::BlobImport {
                workspace: &id(1),
                actor: &id(2),
                id: &id(700),
                sha256: &hash(&large).unwrap(),
                byte_size: &count(large.len() as u64).unwrap(),
                at: &at(),
            },
            &large[..],
        )
        .await
        .unwrap();
        let before = s.diagnostic_bytes().await.unwrap();
        let saved = s
            .read_checkpoint(&id(1), &id(2), &id(300), &id(400))
            .await
            .unwrap();
        let receipt = s.create_backup(&package, &id(800), &at()).await.unwrap();
        let inspected = LocalStore::inspect_backup(&package).await.unwrap();
        assert_eq!(receipt, inspected.receipt);
        assert_eq!(inspected.disposition, "verified_private_backup");
        assert_eq!(before, s.diagnostic_bytes().await.unwrap());
        assert_eq!(
            saved.receipt,
            s.read_checkpoint(&id(1), &id(2), &id(300), &id(400))
                .await
                .unwrap()
                .receipt
        );
        assert!(matches!(
            LocalStore::open(&root.0).await,
            Err(StoreError::WriterOwned)
        ));
        assert!(!package.join("execution.sqlite3-wal").exists());
        assert!(!package.join("supervisor.lock").exists());
        assert!(!package.join("manifest.pending").exists());
        let mut db = SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(package.join("execution.sqlite3"))
                .read_only(true)
                .immutable(true),
        )
        .await
        .unwrap();
        let pointer: String = sqlx::query_scalar("SELECT current_checkpoint_id FROM local_runs")
            .fetch_one(&mut db)
            .await
            .unwrap();
        assert_eq!(pointer, id(400).as_str());
        db.close().await.unwrap();
        s.close().await.unwrap();
    });
}
#[test]
fn backup_paths_states_and_byte_corruption_fail_closed() {
    run(async {
        let root = Root::new();
        let parent = Root::new();
        let mut s = source(&root).await;
        assert!(s
            .create_backup(&root.0.join("inside"), &id(800), &at())
            .await
            .is_err());
        assert!(!root.0.join("inside").exists());
        let existing = parent.0.join("existing");
        fs::DirBuilder::new().mode(0o700).create(&existing).unwrap();
        fs::write(existing.join("sentinel"), b"keep").unwrap();
        assert!(s.create_backup(&existing, &id(800), &at()).await.is_err());
        assert_eq!(fs::read(existing.join("sentinel")).unwrap(), b"keep");
        symlink(&root.0, parent.0.join("link")).unwrap();
        assert!(s
            .create_backup(&parent.0.join("link/new"), &id(800), &at())
            .await
            .is_err());
        sqlx::query("UPDATE local_workspaces SET deletion_generation=1")
            .execute(&mut s.connection)
            .await
            .unwrap();
        let dest = parent.0.join("fenced");
        assert!(s.create_backup(&dest, &id(800), &at()).await.is_err());
        assert!(!dest.exists());
        sqlx::query("UPDATE local_workspaces SET deletion_generation=0")
            .execute(&mut s.connection)
            .await
            .unwrap();
        let path = format!("blobs/{}/vault/{}.bin", id(1).as_str(), id(320).as_str());
        fs::write(root.0.join(&path), b"tampered file\n").unwrap();
        let dest = parent.0.join("corrupt");
        assert!(s.create_backup(&dest, &id(800), &at()).await.is_err());
        assert!(!dest.join("manifest.json").exists());
        assert!(LocalStore::inspect_backup(&dest).await.is_err());
        fs::remove_file(root.0.join(&path)).unwrap();
        let missing = parent.0.join("missing");
        let error = s
            .create_backup(&missing, &id(800), &at())
            .await
            .unwrap_err();
        assert!(
            matches!(error, BackupError::Store(StoreError::Io(ref e)) if e.kind()==std::io::ErrorKind::NotFound)
        );
        assert!(!missing.join("manifest.json").exists());
        s.close().await.unwrap();
    });
}
#[test]
fn inspection_rejects_extra_missing_changed_unsafe_and_schema_mismatch() {
    run(async {
        let root = Root::new();
        let parent = Root::new();
        let package = parent.0.join("snapshot");
        let mut s = source(&root).await;
        s.create_backup(&package, &id(800), &at()).await.unwrap();
        fs::write(package.join("unlisted"), b"extra").unwrap();
        assert!(LocalStore::inspect_backup(&package).await.is_err());
        fs::remove_file(package.join("unlisted")).unwrap();
        let manifest = fs::read(package.join("manifest.json")).unwrap();
        let mut v: serde_json::Value = serde_json::from_slice(&manifest).unwrap();
        v["blobs"][0]["storage_relpath"] = serde_json::json!("../escape");
        fs::write(package.join("manifest.json"), canonical(&v).unwrap()).unwrap();
        assert!(LocalStore::inspect_backup(&package).await.is_err());
        v = serde_json::from_slice(&manifest).unwrap();
        v["journal_schema_version"] = serde_json::json!(99);
        fs::write(package.join("manifest.json"), canonical(&v).unwrap()).unwrap();
        assert!(LocalStore::inspect_backup(&package).await.is_err());
        fs::write(package.join("manifest.json"), &manifest).unwrap();
        v = serde_json::from_slice(&manifest).unwrap();
        v["database_bytes"] = serde_json::json!((MAX_DB + 1).to_string());
        fs::write(package.join("manifest.json"), canonical(&v).unwrap()).unwrap();
        assert!(LocalStore::inspect_backup(&package).await.is_err());
        fs::write(package.join("manifest.json"), &manifest).unwrap();
        fs::set_permissions(
            package.join("manifest.json"),
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        assert!(LocalStore::inspect_backup(&package).await.is_err());
        fs::set_permissions(
            package.join("manifest.json"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        let path = package.join(format!(
            "blobs/{}/vault/{}.bin",
            id(1).as_str(),
            id(320).as_str()
        ));
        fs::remove_file(&path).unwrap();
        assert!(LocalStore::inspect_backup(&package).await.is_err());
        symlink(
            root.0.join(format!(
                "blobs/{}/vault/{}.bin",
                id(1).as_str(),
                id(320).as_str()
            )),
            &path,
        )
        .unwrap();
        assert!(LocalStore::inspect_backup(&package).await.is_err());
        s.close().await.unwrap();
    });
}
pub(super) fn publication_barrier(_destination: &Path, stage: &str) -> Result<(), BackupError> {
    if std::env::var("AVENCREW_BACKUP_CRASH_STAGE").as_deref() == Ok(stage) {
        let marker =
            std::env::var_os("AVENCREW_BACKUP_CRASH_MARKER").ok_or(BackupError::InvalidPath)?;
        fs::write(marker, b"ready")?;
        loop {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    Ok(())
}
#[test]
fn backup_crash_child() {
    let Some(root) = std::env::var_os("AVENCREW_BACKUP_CRASH_ROOT") else {
        return;
    };
    let destination = std::env::var_os("AVENCREW_BACKUP_CRASH_DEST").unwrap();
    run(async {
        let mut s = LocalStore::open(Path::new(&root)).await.unwrap();
        s.create_backup(Path::new(&destination), &id(800), &at())
            .await
            .unwrap();
    });
}
#[test]
fn kill_around_manifest_publication_never_exposes_partial_backup_as_complete() {
    for stage in ["before_publish", "after_publish"] {
        let root = Root::new();
        let parent = Root::new();
        let dest = parent.0.join("snapshot");
        let marker = parent.0.join("reached");
        run(async {
            source(&root).await.close().await.unwrap();
        });
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "journal::backup::tests::backup_crash_child",
                "--nocapture",
            ])
            .env("AVENCREW_BACKUP_CRASH_ROOT", &root.0)
            .env("AVENCREW_BACKUP_CRASH_DEST", &dest)
            .env("AVENCREW_BACKUP_CRASH_STAGE", stage)
            .env("AVENCREW_BACKUP_CRASH_MARKER", &marker)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let start = Clock::now();
        while !marker.exists() {
            if start.elapsed() > Duration::from_secs(15) {
                let _ = child.kill();
                panic!("backup child missed {stage}");
            }
            assert!(child.try_wait().unwrap().is_none());
            std::thread::sleep(Duration::from_millis(10));
        }
        child.kill().unwrap();
        child.wait().unwrap();
        run(async {
            assert_eq!(
                LocalStore::inspect_backup(&dest).await.is_ok(),
                stage == "after_publish"
            );
            LocalStore::open(&root.0)
                .await
                .unwrap()
                .close()
                .await
                .unwrap();
        });
    }
}

#[test]
fn restored_copy_preserves_bytes_but_every_ordinary_boot_is_blocked() {
    run(async {
        let root = Root::new();
        let parent = Root::new();
        let package = parent.0.join("package");
        let restored = parent.0.join("restored");
        let mut s = source(&root).await;
        let backup = s.create_backup(&package, &id(800), &at()).await.unwrap();
        assert!(LocalStore::open(&package).await.is_err());
        assert!(!package.join("supervisor.lock").exists());
        let receipt = LocalStore::restore_backup_quarantined(&package, &restored)
            .await
            .unwrap();
        assert_eq!(receipt.disposition, "quarantined");
        assert_eq!(receipt.manifest_sha256, backup.manifest_sha256);
        assert_eq!(receipt.reasons.len(), 3);
        assert!(restored.join("restore-quarantine.json").exists());
        assert!(restored.join("restore-inspection.json").exists());
        assert!(!restored.join("supervisor.lock").exists());
        assert!(!restored.join("manifest.json").exists());
        assert!(LocalStore::open(&restored).await.is_err());
        assert!(LocalStore::open_existing_for_diagnostics(&restored)
            .await
            .is_err());
        assert_eq!(
            fs::read(package.join("execution.sqlite3")).unwrap(),
            fs::read(restored.join("execution.sqlite3")).unwrap()
        );
        let path = format!("blobs/{}/vault/{}.bin", id(1).as_str(), id(320).as_str());
        assert_eq!(
            fs::read(package.join(&path)).unwrap(),
            fs::read(restored.join(&path)).unwrap()
        );
        assert!(LocalStore::restore_backup_quarantined(&package, &restored)
            .await
            .is_err());
        s.close().await.unwrap();
    });
}
#[test]
fn restore_crash_child() {
    let Some(source) = std::env::var_os("AVENCREW_RESTORE_CRASH_SOURCE") else {
        return;
    };
    let dest = std::env::var_os("AVENCREW_BACKUP_CRASH_DEST").unwrap();
    run(async {
        LocalStore::restore_backup_quarantined(Path::new(&source), Path::new(&dest))
            .await
            .unwrap();
    });
}
#[test]
fn interrupted_restore_is_quarantined_before_data_and_after_publication() {
    for stage in [
        "restore_marker",
        "before_restore_publish",
        "after_restore_publish",
    ] {
        let root = Root::new();
        let parent = Root::new();
        let package = parent.0.join("package");
        let dest = parent.0.join("restored");
        let marker = parent.0.join("reached");
        run(async {
            let mut s = source(&root).await;
            s.create_backup(&package, &id(800), &at()).await.unwrap();
            s.close().await.unwrap();
        });
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "journal::backup::tests::restore_crash_child",
                "--nocapture",
            ])
            .env("AVENCREW_RESTORE_CRASH_SOURCE", &package)
            .env("AVENCREW_BACKUP_CRASH_DEST", &dest)
            .env("AVENCREW_BACKUP_CRASH_STAGE", stage)
            .env("AVENCREW_BACKUP_CRASH_MARKER", &marker)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let start = Clock::now();
        while !marker.exists() {
            if start.elapsed() > Duration::from_secs(15) {
                let _ = child.kill();
                panic!("restore child missed {stage}");
            }
            assert!(child.try_wait().unwrap().is_none());
            std::thread::sleep(Duration::from_millis(10));
        }
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(dest.join("restore-quarantine.json").exists());
        assert_eq!(
            dest.join("restore-inspection.json").exists(),
            stage == "after_restore_publish"
        );
        if stage == "restore_marker" {
            assert!(!dest.join("execution.sqlite3").exists());
        }
        run(async {
            assert!(LocalStore::open(&dest).await.is_err());
            assert!(LocalStore::open_existing_for_diagnostics(&dest)
                .await
                .is_err());
            assert!(LocalStore::inspect_backup(&package).await.is_ok());
        });
    }
}

#[test]
fn rehashed_snapshot_with_wrong_actual_schema_is_still_refused() {
    run(async {
        let root = Root::new();
        let parent = Root::new();
        let package = parent.0.join("package");
        let mut s = source(&root).await;
        s.create_backup(&package, &id(800), &at()).await.unwrap();
        let mut conn = SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(package.join("execution.sqlite3"))
                .journal_mode(sqlx::sqlite::SqliteJournalMode::Delete),
        )
        .await
        .unwrap();
        sqlx::query("PRAGMA user_version=99")
            .execute(&mut conn)
            .await
            .unwrap();
        conn.close().await.unwrap();
        let database = fs::read(package.join("execution.sqlite3")).unwrap();
        let mut m: Manifest =
            serde_json::from_slice(&fs::read(package.join("manifest.json")).unwrap()).unwrap();
        m.database_sha256 = hash(&database).unwrap();
        m.database_bytes = count(database.len() as u64).unwrap();
        fs::write(package.join("manifest.json"), canonical(&m).unwrap()).unwrap();
        assert!(matches!(
            LocalStore::inspect_backup(&package).await,
            Err(BackupError::SchemaMismatch)
        ));
        s.close().await.unwrap();
    });
}
