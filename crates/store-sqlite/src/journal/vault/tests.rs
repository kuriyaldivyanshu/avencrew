use super::super::identity::tests::fixture;
use super::super::tests::{run, Root};
use super::*;
use std::io::Cursor;
use std::os::unix::fs::{symlink, PermissionsExt};
fn id(n: u32) -> DomainId {
    DomainId::new(format!("01900000-0000-7000-8000-{n:012x}")).unwrap()
}
fn hash(b: &[u8]) -> Digest {
    Digest::new(hex(&Sha256::digest(b))).unwrap()
}
async fn setup(root: &Root) -> LocalStore {
    let mut s = LocalStore::open(&root.0).await.unwrap();
    s.bootstrap_standalone(&serde_json::to_vec(&fixture()).unwrap(), "keychain:test")
        .await
        .unwrap();
    s
}
async fn import(s: &mut LocalStore, n: u32, b: &[u8]) -> Result<VaultReceipt, StoreError> {
    s.import_vault_blob(
        BlobImport {
            workspace: &id(1),
            actor: &id(2),
            id: &id(n),
            sha256: &hash(b),
            byte_size: &Counter::new(b.len().to_string()).unwrap(),
            at: &Instant::new("2026-10-08T00:00:00.000000Z").unwrap(),
        },
        Cursor::new(b),
    )
    .await
}
async fn count(s: &mut LocalStore) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM local_blobs WHERE media_type=?")
        .bind(MEDIA)
        .fetch_one(&mut s.connection)
        .await
        .unwrap()
}
#[test]
fn streamed_bytes_retry_reopen_and_verified_read() {
    run(async {
        let root = Root::new();
        let mut s = setup(&root).await;
        let bytes = vec![0x81; 2 * 1024 * 1024 + 11];
        let receipt = import(&mut s, 700, &bytes).await.unwrap();
        assert_eq!(import(&mut s, 700, &bytes).await.unwrap(), receipt);
        assert_eq!(count(&mut s).await, 1);
        assert!(import(&mut s, 700, b"different").await.is_err());
        assert_eq!(count(&mut s).await, 1);
        s.close().await.unwrap();
        let mut s = LocalStore::open(&root.0).await.unwrap();
        let mut f = s.open_vault_blob(&id(1), &id(2), &id(700)).await.unwrap();
        let mut actual = Vec::new();
        f.read_to_end(&mut actual).unwrap();
        assert_eq!(actual, bytes);
        assert_eq!(
            fs::metadata(root.0.join(locator(&id(1), &id(700))))
                .unwrap()
                .mode()
                & 0o777,
            0o600
        );
        s.close().await.unwrap();
    });
}
struct Broken;
impl Read for Broken {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("synthetic local read failure"))
    }
}
#[test]
fn input_failures_and_actual_rollback_publish_no_receipt() {
    run(async {
        let root = Root::new();
        let mut s = setup(&root).await;
        for mode in 0..4 {
            let b = b"payload";
            let expected = hash(if mode == 2 { b"wrong" } else { b });
            let size = Counter::new(match mode {
                0 => "6",
                1 => "8",
                3 => "67108865",
                _ => "7",
            })
            .unwrap();
            assert!(s
                .import_vault_blob(
                    BlobImport {
                        workspace: &id(1),
                        actor: &id(2),
                        id: &id(700),
                        sha256: &expected,
                        byte_size: &size,
                        at: &Instant::new("2026-10-08T00:00:00.000000Z").unwrap()
                    },
                    Cursor::new(b)
                )
                .await
                .is_err());
            assert_eq!(count(&mut s).await, 0);
        }
        assert!(s
            .import_vault_blob(
                BlobImport {
                    workspace: &id(1),
                    actor: &id(2),
                    id: &id(700),
                    sha256: &hash(b""),
                    byte_size: &Counter::new("0").unwrap(),
                    at: &Instant::new("2026-10-08T00:00:00.000000Z").unwrap()
                },
                Broken
            )
            .await
            .is_err());
        sqlx::query("CREATE TEMP TRIGGER fail_vault BEFORE INSERT ON local_blobs WHEN NEW.media_type='application/vnd.avencrew.vault-bytes' BEGIN SELECT RAISE(ABORT,'vault rollback'); END").execute(&mut s.connection).await.unwrap();
        assert!(import(&mut s, 700, b"payload").await.is_err());
        assert_eq!(count(&mut s).await, 0);
        assert!(!root.0.join(locator(&id(1), &id(700))).exists());
        assert!(s.open_vault_blob(&id(1), &id(2), &id(700)).await.is_err());
        sqlx::query("DROP TRIGGER fail_vault")
            .execute(&mut s.connection)
            .await
            .unwrap();
        import(&mut s, 700, b"different retry bytes").await.unwrap();
        assert_eq!(count(&mut s).await, 1);
        assert!(import(&mut s, 20, b"payload").await.is_err()); // Cannot reuse enrollment blob as raw bytes.
        s.close().await.unwrap();
    });
}
#[test]
fn corrupt_missing_unsafe_and_revoked_reads_are_denied() {
    run(async {
        for mode in 0..5 {
            let root = Root::new();
            let mut s = setup(&root).await;
            import(&mut s, 700, b"payload").await.unwrap();
            let p = root.0.join(locator(&id(1), &id(700)));
            match mode {
                0 => fs::write(&p, b"changed").unwrap(),
                1 => fs::remove_file(&p).unwrap(),
                2 => fs::set_permissions(&p, fs::Permissions::from_mode(0o644)).unwrap(),
                3 => {
                    fs::remove_file(&p).unwrap();
                    symlink(root.0.join("journal.sqlite"), &p).unwrap();
                }
                _ => {
                    sqlx::query("UPDATE local_actors SET status='disabled'")
                        .execute(&mut s.connection)
                        .await
                        .unwrap();
                }
            }
            assert!(s.open_vault_blob(&id(1), &id(2), &id(700)).await.is_err());
            s.close().await.unwrap();
        }
    });
}
fn aged(path: &Path) {
    File::open(path)
        .unwrap()
        .set_times(
            fs::FileTimes::new()
                .set_modified(SystemTime::now() - RETENTION - Duration::from_secs(60)),
        )
        .unwrap();
}
#[test]
fn staging_cleanup_is_bounded_and_preserves_references_and_unsafe_names() {
    run(async {
        let root = Root::new();
        let mut s = setup(&root).await;
        import(&mut s, 700, b"payload").await.unwrap();
        let stage = s.vault_staging_folder(&id(1), false).unwrap();
        let folder = stage.parent().unwrap();
        for n in 800..803 {
            let p = stage.join(format!("{}.pending", id(n).as_str()));
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&p)
                .unwrap()
                .write_all(b"staging")
                .unwrap();
            if n != 802 {
                aged(&p);
            }
        }
        let registered = stage.join(format!("{}.pending", id(801).as_str()));
        sqlx::query("INSERT INTO local_blobs (id,workspace_id,created_at_us,owner_actor_id,storage_relpath,sha256,byte_size,media_type,status,erased_at_us) VALUES (?,?,0,?,?,?,7,?,'staging',NULL)").bind(id(801).as_str()).bind(id(1).as_str()).bind(id(2).as_str()).bind(format!("blobs/{}/vault/staging/{}.pending",id(1).as_str(),id(801).as_str())).bind([0u8;32].as_slice()).bind(MEDIA).execute(&mut s.connection).await.unwrap();
        let unsafe_name = stage.join(format!("{}.pending", id(803).as_str()));
        symlink(&registered, &unsafe_name).unwrap();
        let unknown = stage.join("unknown.pending");
        fs::write(&unknown, b"keep").unwrap();
        aged(&unknown);
        let linked = stage.join(format!("{}.pending", id(700).as_str()));
        fs::hard_link(folder.join(format!("{}.bin", id(700).as_str())), &linked).unwrap();
        aged(&linked);
        assert!(s.cleanup_vault_staging(&id(1), &id(2), 0).await.is_err());
        assert!(s.cleanup_vault_staging(&id(1), &id(2), 65).await.is_err());
        assert!(
            s.cleanup_vault_staging(&id(1), &id(2), 1)
                .await
                .unwrap()
                .inspected
                <= 1
        );
        let result = s.cleanup_vault_staging(&id(1), &id(2), 64).await.unwrap();
        assert!(result.removed >= 1);
        assert!(!stage.join(format!("{}.pending", id(800).as_str())).exists());
        assert!(!linked.exists());
        assert!(registered.exists());
        assert!(stage.join(format!("{}.pending", id(802).as_str())).exists());
        assert!(fs::symlink_metadata(unsafe_name)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(unknown.exists());
        assert!(s.open_vault_blob(&id(1), &id(2), &id(700)).await.is_ok());
        s.close().await.unwrap();
    });
}

pub(super) fn commit_barrier(database: &Path, stage: &str) -> Result<(), StoreError> {
    if std::env::var("AVENCREW_VAULT_CRASH_STAGE").as_deref() != Ok(stage)
        || std::env::var("AVENCREW_VAULT_CRASH_ROOT").ok().as_deref()
            != database.parent().and_then(Path::to_str)
    {
        return Ok(());
    }
    let root = database.parent().unwrap();
    fs::write(root.join("vault-crash-pending"), stage)?;
    fs::rename(
        root.join("vault-crash-pending"),
        root.join("vault-crash-ready"),
    )?;
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(30) {
        std::thread::sleep(Duration::from_millis(10));
    }
    Err(invalid("vault crash barrier timed out"))
}
#[test]
fn vault_crash_child() {
    let Ok(root) = std::env::var("AVENCREW_VAULT_CRASH_ROOT") else {
        return;
    };
    run(async {
        let mut s = LocalStore::open(Path::new(&root)).await.unwrap();
        import(&mut s, 700, b"durable payload").await.unwrap();
    });
}
#[test]
fn process_kill_at_actual_commit_preserves_only_verified_references() {
    run(async {
        for stage in ["before_commit", "after_commit"] {
            let root = Root::new();
            let s = setup(&root).await;
            s.close().await.unwrap();
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "journal::vault::tests::vault_crash_child",
                    "--nocapture",
                ])
                .env("AVENCREW_VAULT_CRASH_ROOT", &root.0)
                .env("AVENCREW_VAULT_CRASH_STAGE", stage)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap();
            let start = std::time::Instant::now();
            while !root.0.join("vault-crash-ready").exists() {
                if let Some(status) = child.try_wait().unwrap() {
                    panic!("vault child exited before barrier: {status}");
                }
                if start.elapsed() > Duration::from_secs(15) {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("vault child timed out");
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            child.kill().unwrap();
            child.wait().unwrap();
            let mut s = LocalStore::open(&root.0).await.unwrap();
            assert_eq!(count(&mut s).await, i64::from(stage == "after_commit"));
            assert_eq!(
                s.open_vault_blob(&id(1), &id(2), &id(700)).await.is_ok(),
                stage == "after_commit"
            );
            import(&mut s, 700, b"durable payload").await.unwrap();
            assert_eq!(count(&mut s).await, 1);
            let mut f = s.open_vault_blob(&id(1), &id(2), &id(700)).await.unwrap();
            let mut actual = Vec::new();
            f.read_to_end(&mut actual).unwrap();
            assert_eq!(actual, b"durable payload");
            s.close().await.unwrap();
        }
    });
}
