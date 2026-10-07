use super::*;
use std::future::Future;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
const W: &str = "01900000-0000-7000-8000-000000000001";
const A: &str = "01900000-0000-7000-8000-000000000002";
const B: &str = "01900000-0000-7000-8000-000000000003";
const T: &str = "01900000-0000-7000-8000-000000000004";
const R: &str = "01900000-0000-7000-8000-000000000005";

pub(super) struct Root(pub(super) PathBuf);
impl Root {
    pub(super) fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "avencrew-store-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
pub(super) fn run(future: impl Future<Output = ()>) {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future);
}
async fn seed(conn: &mut SqliteConnection) {
    sqlx::query("INSERT INTO local_workspaces VALUES (?,?,0,'test','public-key','active',0,0,0)")
        .bind(W)
        .bind(W)
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query("INSERT INTO local_actors VALUES (?,?,0,'human','owner','active',NULL)")
        .bind(A)
        .bind(W)
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query("INSERT INTO local_blobs VALUES (?,?,0,?,'blobs/test',zeroblob(32),0,'application/json','verified',NULL)").bind(B).bind(W).bind(A).execute(&mut *conn).await.unwrap();
}
async fn task(conn: &mut SqliteConnection, task: &str, revision: &str) {
    sqlx::query("INSERT INTO local_tasks VALUES (?,?,0,?,NULL,'local','open',0)")
        .bind(task)
        .bind(W)
        .bind(A)
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query("INSERT INTO local_task_revisions VALUES (?,?,0,?,1,1,?,zeroblob(32))")
        .bind(revision)
        .bind(W)
        .bind(task)
        .bind(B)
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query("UPDATE local_tasks SET current_revision_id=? WHERE workspace_id=? AND id=?")
        .bind(revision)
        .bind(W)
        .bind(task)
        .execute(&mut *conn)
        .await
        .unwrap();
}

#[test]
fn fresh_boot_reopen_layout_and_permissions() {
    run(async {
        let root = Root::new();
        let store = LocalStore::open(&root.0).await.unwrap();
        assert_eq!(store.status().schema_version, 2);
        assert_eq!(store.status().domain_tables, 19);
        for file in [
            "execution.sqlite3",
            "execution.sqlite3-wal",
            "execution.sqlite3-shm",
            "supervisor.lock",
        ] {
            assert_eq!(
                fs::metadata(root.0.join(file)).unwrap().mode() & 0o777,
                0o600
            );
        }
        store.close().await.unwrap();
        let mut store = LocalStore::open(&root.0).await.unwrap();
        let future: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pragma_table_info('local_workspaces') WHERE name='server_url'",
        )
        .fetch_one(&mut store.connection)
        .await
        .unwrap();
        assert_eq!(future, 0);
        let hashes: i64 = sqlx::query_scalar("SELECT min(length(checksum)) FROM _sqlx_migrations")
            .fetch_one(&mut store.connection)
            .await
            .unwrap();
        assert_eq!(hashes, 48);
        let strict: i64 = sqlx::query_scalar(
            "SELECT strict FROM pragma_table_list WHERE name='_sqlx_migrations'",
        )
        .fetch_one(&mut store.connection)
        .await
        .unwrap();
        assert_eq!(strict, 0);
        store.close().await.unwrap();
    });
}

#[test]
fn stage_upgrade_snapshots_committed_wal_and_preserves_rows() {
    run(async {
        let root = Root::new();
        let mut store = LocalStore::open_to(&root.0, 1).await.unwrap();
        seed(&mut store.connection).await;
        store.close().await.unwrap();
        let mut store = LocalStore::open(&root.0).await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM local_actors")
            .fetch_one(&mut store.connection)
            .await
            .unwrap();
        assert_eq!(count, 1);
        let backup = fs::read_dir(root.0.join("backups"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let mut conn = SqliteConnection::connect_with(
            &SqliteConnectOptions::new().filename(backup).read_only(true),
        )
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("PRAGMA user_version")
                .fetch_one(&mut conn)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM local_actors")
                .fetch_one(&mut conn)
                .await
                .unwrap(),
            1
        );
        conn.close().await.unwrap();
        store.close().await.unwrap();
    });
}

#[test]
fn writer_ownership_and_unsafe_paths_fail_closed() {
    run(async {
        let root = Root::new();
        let store = LocalStore::open(&root.0).await.unwrap();
        assert!(matches!(
            LocalStore::open(&root.0).await,
            Err(StoreError::WriterOwned)
        ));
        store.close().await.unwrap();
        let second = LocalStore::open(&root.0).await.unwrap();
        second.close().await.unwrap();
        let link = root.0.join("alias");
        symlink(root.0.join("execution.sqlite3"), &link).unwrap();
        fs::remove_file(root.0.join("execution.sqlite3")).unwrap();
        fs::rename(&link, root.0.join("execution.sqlite3")).unwrap();
        assert!(LocalStore::open(&root.0).await.is_err());
        assert!(LocalStore::open(Path::new("relative-runtime"))
            .await
            .is_err());
        let public = Root::new();
        fs::set_permissions(&public.0, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(LocalStore::open(&public.0).await.is_err());
    });
}

#[test]
fn strict_checks_same_parent_and_partial_attempt_index() {
    run(async {
        let root = Root::new();
        let mut store = LocalStore::open(&root.0).await.unwrap();
        let c = &mut store.connection;
        seed(c).await;
        assert!(sqlx::query("UPDATE local_blobs SET byte_size='text'")
            .execute(&mut *c)
            .await
            .is_err());
        assert!(sqlx::query("UPDATE local_blobs SET sha256=zeroblob(31)")
            .execute(&mut *c)
            .await
            .is_err());
        assert!(
            sqlx::query("UPDATE local_blobs SET storage_relpath='../secret'")
                .execute(&mut *c)
                .await
                .is_err()
        );
        assert!(sqlx::query("UPDATE local_blobs SET owner_actor_id=?")
            .bind(T)
            .execute(&mut *c)
            .await
            .is_err());
        task(c, T, R).await;
        assert!(sqlx::query("UPDATE local_task_revisions SET version_no=2")
            .execute(&mut *c)
            .await
            .is_err());
        assert!(sqlx::query("UPDATE local_tasks SET id=? WHERE id=?")
            .bind(B)
            .bind(T)
            .execute(&mut *c)
            .await
            .is_err());
        let t2 = "01900000-0000-7000-8000-000000000006";
        let r2 = "01900000-0000-7000-8000-000000000007";
        task(c, t2, r2).await;
        let mut tx = c.begin().await.unwrap();
        sqlx::query("UPDATE local_tasks SET current_revision_id=? WHERE id=?")
            .bind(r2)
            .bind(T)
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(tx.commit().await.is_err());
        let device = "01900000-0000-7000-8000-000000000008";
        let run = "01900000-0000-7000-8000-000000000009";
        sqlx::query(
            "INSERT INTO local_devices VALUES (?,?,0,?,'key','keychain-reference','active')",
        )
        .bind(device)
        .bind(W)
        .bind(A)
        .execute(&mut *c)
        .await
        .unwrap();
        sqlx::query("INSERT INTO local_runs VALUES (?,?,0,?,?,NULL,'local',?,'queued',0,1,0,0,0,NULL,0,NULL)").bind(run).bind(W).bind(T).bind(R).bind(device).execute(&mut *c).await.unwrap();
        for (id, epoch, expected) in [
            ("01900000-0000-7000-8000-00000000000a", 1, true),
            ("01900000-0000-7000-8000-00000000000b", 2, false),
        ] {
            let result=sqlx::query("INSERT INTO local_attempts VALUES (?,?,0,?,?,'local','claimed',printf('%064d',0),1,NULL,NULL,NULL,'boot',0,NULL)").bind(id).bind(W).bind(run).bind(epoch).execute(&mut *c).await;
            assert_eq!(result.is_ok(), expected, "attempt result: {result:?}");
        }
        assert!(sqlx::query("UPDATE local_runs SET status='completed'")
            .execute(&mut *c)
            .await
            .is_err());
        sqlx::query("UPDATE local_runs SET status='cancelling',cancellation_requested=1")
            .execute(&mut *c)
            .await
            .unwrap();
        assert!(
            sqlx::query("UPDATE local_runs SET cancellation_requested=0")
                .execute(&mut *c)
                .await
                .is_err()
        );
        sqlx::query("UPDATE local_runs SET status='cancelled'")
            .execute(&mut *c)
            .await
            .unwrap();
        assert!(sqlx::query("UPDATE local_runs SET status='queued'")
            .execute(&mut *c)
            .await
            .is_err());
        store.close().await.unwrap();
    });
}

#[test]
fn newer_tampered_and_checksum_mismatched_databases_are_refused() {
    run(async {
        for sql in [
            "PRAGMA user_version=3",
            "DROP INDEX local_devices_actor_id_fk",
            "UPDATE _sqlx_migrations SET checksum=zeroblob(48) WHERE version=1",
        ] {
            let root = Root::new();
            let mut store = LocalStore::open(&root.0).await.unwrap();
            sqlx::raw_sql(sql)
                .execute(&mut store.connection)
                .await
                .unwrap();
            store.close().await.unwrap();
            assert!(
                LocalStore::open(&root.0).await.is_err(),
                "accepted tampered database: {sql}"
            );
        }
    });
}

#[test]
fn failed_migration_and_full_disk_transaction_leave_no_success() {
    run(async {
        let root = Root::new();
        let mut store = LocalStore::open_to(&root.0, 1).await.unwrap();
        let c = &mut store.connection;
        let sql="CREATE TABLE failure_probe(id INTEGER) STRICT; PRAGMA user_version=2; SELECT * FROM missing_probe;";
        let bad = Migrator {
            migrations: Cow::Owned(vec![Migration::new(
                2,
                "failure probe".into(),
                MigrationType::Simple,
                sql.into_sql_str(),
                false,
            )]),
            ignore_missing: true,
            ..Migrator::DEFAULT
        };
        assert!(bad.run(&mut *c).await.is_err());
        assert_eq!(
            sqlx::query_scalar::<_, i64>("PRAGMA user_version")
                .fetch_one(&mut *c)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM sqlite_schema WHERE name='failure_probe'"
            )
            .fetch_one(&mut *c)
            .await
            .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM _sqlx_migrations WHERE version=2")
                .fetch_one(&mut *c)
                .await
                .unwrap(),
            0
        );
        seed(c).await;
        sqlx::query("PRAGMA max_page_count=1")
            .execute(&mut *c)
            .await
            .unwrap();
        let mut tx = c.begin().await.unwrap();
        let result=sqlx::query("INSERT INTO local_actors VALUES (?,?,0,'human',CAST(zeroblob(1048576) AS TEXT),'active',NULL)").bind(T).bind(W).execute(&mut *tx).await;
        assert_eq!(
            result
                .unwrap_err()
                .as_database_error()
                .unwrap()
                .code()
                .as_deref(),
            Some("13")
        );
        let _ = tx.rollback().await;
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM local_actors WHERE id=?")
                .bind(T)
                .fetch_one(&mut *c)
                .await
                .unwrap(),
            0
        );
        store.close().await.unwrap();
    });
}

#[test]
fn database_busy_is_bounded_and_no_row_is_acknowledged() {
    run(async {
        let root = Root::new();
        let mut store = LocalStore::open(&root.0).await.unwrap();
        seed(&mut store.connection).await;
        // Deliberately bypass the public ownership boundary to simulate a foreign
        // SQLite writer. Production callers cannot obtain either connection.
        let mut foreign = SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(&store.status.database_path)
                .foreign_keys(true),
        )
        .await
        .unwrap();
        let tx = foreign.begin_with("BEGIN IMMEDIATE").await.unwrap();
        let started = std::time::Instant::now();
        assert!(
            sqlx::query("UPDATE local_workspaces SET name='should not commit'")
                .execute(&mut store.connection)
                .await
                .is_err()
        );
        assert!(started.elapsed() >= Duration::from_secs(4));
        assert!(started.elapsed() < Duration::from_secs(10));
        tx.rollback().await.unwrap();
        foreign.close().await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT name FROM local_workspaces")
                .fetch_one(&mut store.connection)
                .await
                .unwrap(),
            "test"
        );
        store.close().await.unwrap();
    });
}

#[test]
fn cross_tenant_and_branch_cycles_are_rejected() {
    run(async {
        let root = Root::new();
        let mut store = LocalStore::open(&root.0).await.unwrap();
        let c = &mut store.connection;
        seed(c).await;
        sqlx::query(
            "INSERT INTO local_workspaces VALUES (?,?,0,'other','other-key','active',0,0,0)",
        )
        .bind(T)
        .bind(T)
        .execute(&mut *c)
        .await
        .unwrap();
        sqlx::query("INSERT INTO local_actors VALUES (?,?,0,'human','other','active',NULL)")
            .bind(R)
            .bind(T)
            .execute(&mut *c)
            .await
            .unwrap();
        assert!(
            sqlx::query("UPDATE local_blobs SET owner_actor_id=? WHERE workspace_id=?")
                .bind(R)
                .bind(W)
                .execute(&mut *c)
                .await
                .is_err()
        );
        for session in [T, R] {
            sqlx::query("INSERT INTO local_sessions VALUES (?,?,0,?,'session','open')")
                .bind(session)
                .bind(W)
                .bind(A)
                .execute(&mut *c)
                .await
                .unwrap();
        }
        let p = "01900000-0000-7000-8000-000000000006";
        let q = "01900000-0000-7000-8000-000000000007";
        sqlx::query("INSERT INTO local_session_branches VALUES (?,?,0,?,NULL,NULL,0)")
            .bind(p)
            .bind(W)
            .bind(T)
            .execute(&mut *c)
            .await
            .unwrap();
        let mut tx = c.begin().await.unwrap();
        sqlx::query("INSERT INTO local_session_branches VALUES (?,?,0,?,?,NULL,0)")
            .bind(q)
            .bind(W)
            .bind(R)
            .bind(p)
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(tx.commit().await.is_err());
        let mut tx = c.begin().await.unwrap();
        sqlx::query("INSERT INTO local_session_branches VALUES (?,?,0,?,?,NULL,0)")
            .bind(q)
            .bind(W)
            .bind(T)
            .bind(B)
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(
            sqlx::query("INSERT INTO local_session_branches VALUES (?,?,0,?,?,NULL,0)")
                .bind(B)
                .bind(W)
                .bind(T)
                .bind(q)
                .execute(&mut *tx)
                .await
                .is_err()
        );
        tx.rollback().await.unwrap();
        store.close().await.unwrap();
    });
}

#[test]
fn crash_child() {
    let Ok(root) = std::env::var("AVENCREW_STORE_TEST_ROOT") else {
        return;
    };
    run(async {
        let mut store = LocalStore::open(Path::new(&root)).await.unwrap();
        let committed = std::env::var("AVENCREW_STORE_TEST_MODE").unwrap() == "after";
        let mut tx = store.connection.begin().await.unwrap();
        sqlx::query(
            "INSERT INTO local_workspaces VALUES (?,?,0,'crash-probe','key','active',0,0,0)",
        )
        .bind(W)
        .bind(W)
        .execute(&mut *tx)
        .await
        .unwrap();
        if committed {
            tx.commit().await.unwrap();
        }
        File::create_new(Path::new(&root).join("test-ready")).unwrap();
        // Parent kills this process; no graceful rollback/close is exercised.
        std::thread::sleep(Duration::from_secs(30));
    });
}

#[test]
fn process_death_releases_lock_and_preserves_only_committed_rows() {
    run(async {
        for mode in ["before", "after"] {
            let root = Root::new();
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "journal::tests::crash_child", "--nocapture"])
                .env("AVENCREW_STORE_TEST_ROOT", &root.0)
                .env("AVENCREW_STORE_TEST_MODE", mode)
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap();
            let started = std::time::Instant::now();
            while !root.0.join("test-ready").exists() && started.elapsed() < Duration::from_secs(10)
            {
                if let Some(status) = child.try_wait().unwrap() {
                    panic!("crash child exited before ready: {status}");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            let ready = root.0.join("test-ready").exists();
            let owned = if ready {
                matches!(
                    LocalStore::open(&root.0).await,
                    Err(StoreError::WriterOwned)
                )
            } else {
                false
            };
            child.kill().unwrap();
            child.wait().unwrap();
            assert!(ready && owned);
            let mut store = LocalStore::open(&root.0).await.unwrap();
            let count: i64 = sqlx::query_scalar("SELECT count(*) FROM local_workspaces")
                .fetch_one(&mut store.connection)
                .await
                .unwrap();
            assert_eq!(count, i64::from(mode == "after"));
            store.close().await.unwrap();
        }
    });
}

#[test]
fn budget_overlap_denied_but_real_overruns_remain_recordable() {
    run(async {
        let root = Root::new();
        let mut store = LocalStore::open(&root.0).await.unwrap();
        let c = &mut store.connection;
        seed(c).await;
        sqlx::query(
            "INSERT INTO local_budget_periods VALUES (?,?,0,'workspace','money','USD',0,10,0,0,0)",
        )
        .bind(T)
        .bind(W)
        .execute(&mut *c)
        .await
        .unwrap();
        assert!(sqlx::query(
            "INSERT INTO local_budget_periods VALUES (?,?,0,'workspace','money','USD',5,15,0,0,0)"
        )
        .bind(R)
        .bind(W)
        .execute(&mut *c)
        .await
        .is_err());
        sqlx::query(
            "INSERT INTO local_budget_periods VALUES (?,?,0,'workspace','money','USD',10,20,0,0,0)",
        )
        .bind(R)
        .bind(W)
        .execute(&mut *c)
        .await
        .unwrap();
        sqlx::query("UPDATE local_budget_periods SET settled_micro=100 WHERE id=?")
            .bind(T)
            .execute(&mut *c)
            .await
            .unwrap();
        store.close().await.unwrap();
    });
}
