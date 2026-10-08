use super::super::identity::tests::fixture as identity_fixture;
use super::super::publication::tests::{allocations, fixture};
use super::super::tests::{run, Root};
use super::super::BundlePublication;
use super::*;
use avencrew_contracts::bundles::Purpose;
use serde_json::{json, Value};
fn id(n: u32) -> DomainId {
    DomainId::new(format!("01900000-0000-7000-8000-{n:012x}")).unwrap()
}
fn at() -> Instant {
    Instant::new("2026-10-08T01:00:00.000000Z").unwrap()
}
pub(crate) async fn setup(root: &Root) -> LocalStore {
    let mut s = LocalStore::open(&root.0).await.unwrap();
    s.bootstrap_standalone(
        &serde_json::to_vec(&identity_fixture()).unwrap(),
        "keychain:test",
    )
    .await
    .unwrap();
    let v = fixture(Purpose::Task, false);
    let bytes = canonical(&v).unwrap();
    let records = allocations(&v);
    s.publish_bundle(BundlePublication {
        workspace: &id(1),
        actor: &id(2),
        bundle_blob_id: &id(900),
        bytes: &bytes,
        sha256: digest(&bytes),
        record_blobs: &records,
        payloads: &[],
    })
    .await
    .unwrap();
    s
}
pub(crate) async fn register(s: &mut LocalStore) -> Result<LocalRunReceipt, ControlError> {
    s.register_local_run(LocalRunRegistration {
        workspace: &id(1),
        actor: &id(2),
        task: &id(100),
        task_revision: &id(103),
        run: &id(300),
        event: &id(301),
        queue: &id(302),
        payload_blob: &id(303),
        at: &at(),
    })
    .await
}
fn command(n: u32, kind: &str, version: u32) -> Value {
    json!({"kind":kind,"command_id":id(n),"run_id":id(300),"idempotency_key":format!("control-{n}"),"payload":match kind {"stop"=>json!({"reason":"Requested stop"}),"pause"=>json!({"reason":"Pause at safe boundary","expected_revision":version.to_string()}),_=>json!({"text":"Keep the requested outcome","expected_revision":version.to_string()})}})
}
async fn accept(s: &mut LocalStore, v: &Value) -> Result<CommandReceipt, ControlError> {
    let command_id = DomainId::new(v["command_id"].as_str().unwrap()).unwrap();
    let n = u32::from_str_radix(&command_id.as_str()[24..], 16).unwrap();
    accept_bytes(s, &serde_json::to_vec(v).unwrap(), n).await
}
async fn accept_bytes(
    s: &mut LocalStore,
    bytes: &[u8],
    n: u32,
) -> Result<CommandReceipt, ControlError> {
    s.accept_command(CommandAcceptance {
        workspace: &id(1),
        actor: &id(2),
        bytes,
        event: &id(n + 10000),
        queue: &id(n + 20000),
        command_blob: &id(n + 30000),
        event_blob: &id(n + 40000),
        at: &at(),
    })
    .await
}
async fn counts(s: &mut LocalStore) -> (i64, i64, i64) {
    let c = sqlx::query_scalar("SELECT count(*) FROM local_commands")
        .fetch_one(&mut s.connection)
        .await
        .unwrap();
    let e = sqlx::query_scalar("SELECT count(*) FROM local_events")
        .fetch_one(&mut s.connection)
        .await
        .unwrap();
    let q = sqlx::query_scalar("SELECT count(*) FROM local_work_queue")
        .fetch_one(&mut s.connection)
        .await
        .unwrap();
    (c, e, q)
}
#[test]
fn registration_and_acceptance_reopen_and_retry_keep_original_receipts() {
    run(async {
        let root = Root::new();
        let mut s = setup(&root).await;
        let r = register(&mut s).await.unwrap();
        assert_eq!(register(&mut s).await.unwrap(), r);
        let c = command(400, "steer", 1);
        let receipt = accept(&mut s, &c).await.unwrap();
        assert_eq!(counts(&mut s).await, (1, 2, 2));
        let CommandReceipt::Accepted {
            accepted_seq,
            event_seq,
            row_version,
            ..
        } = &receipt
        else {
            panic!()
        };
        assert_eq!(
            (accepted_seq.value(), event_seq.value(), row_version.value()),
            (1, 2, 2)
        );
        let later = command(401, "pause", 2);
        accept(&mut s, &later).await.unwrap();
        assert_eq!(accept(&mut s, &c).await.unwrap(), receipt);
        sqlx::query("UPDATE local_work_queue SET due_at_us=due_at_us+100 WHERE dedupe_key=?")
            .bind("command:01900000-0000-7000-8000-000000000190")
            .execute(&mut s.connection)
            .await
            .unwrap();
        assert_eq!(accept(&mut s, &c).await.unwrap(), receipt);
        let row = run_row(&mut s.connection, &id(1), &id(300), &id(2))
            .await
            .unwrap();
        assert_eq!(
            (row.version, row.accepted, row.applied, row.event),
            (3, 2, 0, 3)
        );
        assert_eq!(row.status, "queued");
        assert_eq!(register(&mut s).await.unwrap(), r);
        s.close().await.unwrap();
        let mut s = LocalStore::open(&root.0).await.unwrap();
        assert_eq!(accept(&mut s, &c).await.unwrap(), receipt);
        assert_eq!(counts(&mut s).await, (2, 3, 3));
        s.close().await.unwrap();
    });
}
#[test]
fn conflicts_and_hostile_wire_inputs_do_not_allocate_sequences() {
    run(async {
        let root = Root::new();
        let mut s = setup(&root).await;
        register(&mut s).await.unwrap();
        let c = command(400, "steer", 1);
        accept(&mut s, &c).await.unwrap();
        let mut changed = c.clone();
        changed["payload"]["text"] = json!("Changed bytes");
        assert!(matches!(
            accept(&mut s, &changed).await,
            Err(ControlError::IdempotencyMismatch)
        ));
        let mut changed = c.clone();
        changed["command_id"] = json!(id(401));
        assert!(matches!(
            accept(&mut s, &changed).await,
            Err(ControlError::IdempotencyMismatch)
        ));
        let mut changed = c.clone();
        changed["idempotency_key"] = json!("different-key");
        assert!(matches!(
            accept(&mut s, &changed).await,
            Err(ControlError::Conflict)
        ));
        assert!(matches!(
            accept(&mut s, &command(402, "pause", 1)).await,
            Err(ControlError::Conflict)
        ));
        for bad in [
            json!({"kind":"steer"}),
            {
                let mut v = command(403, "steer", 2);
                v["actor_id"] = json!(id(2));
                v
            },
            {
                let mut v = command(403, "steer", 2);
                v["payload"]["expected_revision"] = json!(2);
                v
            },
            {
                let mut v = command(403, "steer", 2);
                v["payload"]["expected_revision"] = json!("02");
                v
            },
        ] {
            assert!(matches!(
                accept_bytes(&mut s, &serde_json::to_vec(&bad).unwrap(), 403).await,
                Err(ControlError::InvalidCommand)
            ));
        }
        let bytes = serde_json::to_string(&command(403, "steer", 2)).unwrap();
        let duplicate = bytes.replacen("{", "{\"kind\":\"pause\",", 1);
        assert!(matches!(
            accept_bytes(&mut s, duplicate.as_bytes(), 403).await,
            Err(ControlError::InvalidCommand)
        ));
        assert!(matches!(
            accept_bytes(&mut s, &vec![b' '; MAX_COMMAND_BYTES + 1], 403).await,
            Err(ControlError::InvalidCommand)
        ));
        let resume = json!({"kind":"resume","command_id":id(404),"run_id":id(300),"idempotency_key":"resume","payload":{"expected_revision":"2","checkpoint_id":id(405)}});
        assert!(matches!(
            accept(&mut s, &resume).await,
            Err(ControlError::Unsupported)
        ));
        assert_eq!(counts(&mut s).await, (1, 2, 2));
        sqlx::query("UPDATE local_runs SET row_version=9223372036854775807 WHERE id=?")
            .bind(id(300).as_str())
            .execute(&mut s.connection)
            .await
            .unwrap();
        assert!(accept(&mut s, &command(406, "stop", 1)).await.is_err());
        assert_eq!(counts(&mut s).await, (1, 2, 2));
        s.close().await.unwrap();
    });
}
#[test]
fn authority_owner_revocation_and_wrong_revision_fail_closed() {
    run(async {
        let root = Root::new();
        let mut s = setup(&root).await;
        register(&mut s).await.unwrap();
        let bytes = serde_json::to_vec(&command(400, "stop", 1)).unwrap();
        assert!(s
            .accept_command(CommandAcceptance {
                workspace: &id(1),
                actor: &id(3),
                bytes: &bytes,
                event: &id(10400),
                queue: &id(20400),
                command_blob: &id(30400),
                event_blob: &id(40400),
                at: &at()
            })
            .await
            .is_err());
        for authority in ["server", "sealed", "transfer_staging"] {
            sqlx::query("UPDATE local_runs SET authority=?,authority_device_id=? WHERE id=?")
                .bind(authority)
                .bind(if authority == "server" {
                    None
                } else {
                    Some(id(5).as_str().to_owned())
                })
                .bind(id(300).as_str())
                .execute(&mut s.connection)
                .await
                .unwrap();
            assert!(matches!(
                accept(&mut s, &command(400, "stop", 1)).await,
                Err(ControlError::StaleAuthority)
            ));
        }
        sqlx::query("UPDATE local_runs SET authority='local',authority_device_id=? WHERE id=?")
            .bind(id(5).as_str())
            .bind(id(300).as_str())
            .execute(&mut s.connection)
            .await
            .unwrap();
        sqlx::query("INSERT INTO local_devices VALUES (?,?,0,?,'efab','keychain:other','active')")
            .bind(id(550).as_str())
            .bind(id(1).as_str())
            .bind(id(2).as_str())
            .execute(&mut s.connection)
            .await
            .unwrap();
        sqlx::query("UPDATE local_runs SET authority_device_id=? WHERE id=?")
            .bind(id(550).as_str())
            .bind(id(300).as_str())
            .execute(&mut s.connection)
            .await
            .unwrap();
        assert!(matches!(
            accept(&mut s, &command(400, "stop", 1)).await,
            Err(ControlError::StaleAuthority)
        ));
        sqlx::query("UPDATE local_runs SET authority_device_id=? WHERE id=?")
            .bind(id(5).as_str())
            .bind(id(300).as_str())
            .execute(&mut s.connection)
            .await
            .unwrap();
        assert!(s
            .register_local_run(LocalRunRegistration {
                workspace: &id(1),
                actor: &id(2),
                task: &id(100),
                task_revision: &id(104),
                run: &id(305),
                event: &id(306),
                queue: &id(307),
                payload_blob: &id(308),
                at: &at()
            })
            .await
            .is_err());
        sqlx::query("UPDATE local_actors SET status='disabled' WHERE id=?")
            .bind(id(2).as_str())
            .execute(&mut s.connection)
            .await
            .unwrap();
        assert!(accept(&mut s, &command(400, "stop", 1)).await.is_err());
        assert_eq!(counts(&mut s).await, (0, 1, 1));
        s.close().await.unwrap();
    });
}
#[test]
fn stop_is_sticky_preempts_pending_dispatch_and_is_not_final_cancellation() {
    run(async {
        for status in [
            "queued",
            "preparing",
            "running",
            "waiting",
            "paused",
            "transferring",
            "verifying",
            "recovering",
            "blocked",
            "cancelling",
        ] {
            let root = Root::new();
            let mut s = setup(&root).await;
            register(&mut s).await.unwrap();
            // Test-only seeding of each legal source state, not a production transition API.
            let guard: String = sqlx::query_scalar(
                "SELECT sql FROM sqlite_schema WHERE name='local_runs_state_guard'",
            )
            .fetch_one(&mut s.connection)
            .await
            .unwrap();
            sqlx::query("DROP TRIGGER local_runs_state_guard")
                .execute(&mut s.connection)
                .await
                .unwrap();
            sqlx::query("UPDATE local_runs SET status=? WHERE id=?")
                .bind(status)
                .bind(id(300).as_str())
                .execute(&mut s.connection)
                .await
                .unwrap();
            sqlx::query(sqlx::AssertSqlSafe(guard.as_str()))
                .execute(&mut s.connection)
                .await
                .unwrap();
            sqlx::query("INSERT INTO local_work_queue VALUES (?,?,0,?,'dispatch','dispatch-test',0,'claimed','claim',100,1,NULL,NULL)").bind(id(500).as_str()).bind(id(1).as_str()).bind(id(300).as_str()).execute(&mut s.connection).await.unwrap();
            accept(&mut s, &command(400, "stop", 1)).await.unwrap();
            let r = run_row(&mut s.connection, &id(1), &id(300), &id(2))
                .await
                .unwrap();
            assert!(r.cancelled);
            assert_eq!(r.status, "cancelling");
            assert_eq!(r.applied, 0);
            let q = sqlx::query(
                "SELECT state,claim_token,lease_until_us FROM local_work_queue WHERE id=?",
            )
            .bind(id(500).as_str())
            .fetch_one(&mut s.connection)
            .await
            .unwrap();
            assert_eq!(q.try_get::<String, _>("state").unwrap(), "cancelled");
            assert!(q
                .try_get::<Option<String>, _>("claim_token")
                .unwrap()
                .is_none());
            assert!(q
                .try_get::<Option<i64>, _>("lease_until_us")
                .unwrap()
                .is_none());
            assert!(matches!(
                accept(&mut s, &command(401, "steer", 2)).await,
                Err(ControlError::Cancelled)
            ));
            s.close().await.unwrap();
        }
    });
}
#[test]
fn terminal_stop_is_noop_but_prior_retry_returns_original_receipt() {
    run(async {
        for status in ["completed", "cancelled", "failed"] {
            let root = Root::new();
            let mut s = setup(&root).await;
            register(&mut s).await.unwrap();
            let c = command(400, "steer", 1);
            let receipt = accept(&mut s, &c).await.unwrap();
            let guard: String = sqlx::query_scalar(
                "SELECT sql FROM sqlite_schema WHERE name='local_runs_state_guard'",
            )
            .fetch_one(&mut s.connection)
            .await
            .unwrap();
            sqlx::query("DROP TRIGGER local_runs_state_guard")
                .execute(&mut s.connection)
                .await
                .unwrap();
            sqlx::query("UPDATE local_runs SET status=? WHERE id=?")
                .bind(status)
                .bind(id(300).as_str())
                .execute(&mut s.connection)
                .await
                .unwrap();
            sqlx::query(sqlx::AssertSqlSafe(guard.as_str()))
                .execute(&mut s.connection)
                .await
                .unwrap();
            assert!(matches!(
                accept(&mut s, &command(401, "stop", 1)).await.unwrap(),
                CommandReceipt::Terminal { .. }
            ));
            assert_eq!(accept(&mut s, &c).await.unwrap(), receipt);
            assert_eq!(counts(&mut s).await, (1, 2, 2));
            s.close().await.unwrap();
        }
    });
}
#[test]
fn database_abort_rolls_back_command_event_queue_and_stop_intent() {
    run(async {
        let root = Root::new();
        let mut s = setup(&root).await;
        register(&mut s).await.unwrap();
        sqlx::query("CREATE TEMP TRIGGER refuse_intent BEFORE INSERT ON local_work_queue BEGIN SELECT RAISE(ABORT,'test refusal'); END").execute(&mut s.connection).await.unwrap();
        let c = command(400, "stop", 1);
        assert!(accept(&mut s, &c).await.is_err());
        assert_eq!(counts(&mut s).await, (0, 1, 1));
        let r = run_row(&mut s.connection, &id(1), &id(300), &id(2))
            .await
            .unwrap();
        assert!(!r.cancelled);
        assert_eq!((r.version, r.accepted, r.event), (1, 0, 1));
        sqlx::query("DROP TRIGGER refuse_intent")
            .execute(&mut s.connection)
            .await
            .unwrap();
        accept(&mut s, &c).await.unwrap();
        assert_eq!(counts(&mut s).await, (1, 2, 2));
        s.close().await.unwrap();
    });
}
#[test]
fn corrupt_bytes_and_divergent_queue_projections_block_receipt_recovery() {
    run(async {
        for mode in 0..2 {
            let root = Root::new();
            let mut s = setup(&root).await;
            register(&mut s).await.unwrap();
            let c = command(400, "steer", 1);
            accept(&mut s, &c).await.unwrap();
            if mode == 0 {
                std::fs::write(root.0.join(relative(&id(1), &id(30400))), b"corrupt").unwrap();
            } else {
                sqlx::query("UPDATE local_work_queue SET spec_blob_id=? WHERE dedupe_key=?")
                    .bind(id(303).as_str())
                    .bind(format!("command:{}", id(400).as_str()))
                    .execute(&mut s.connection)
                    .await
                    .unwrap();
            }
            assert!(accept(&mut s, &c).await.is_err());
            assert_eq!(counts(&mut s).await, (1, 2, 2));
            s.close().await.unwrap();
        }
    });
}
#[test]
fn control_crash_child() {
    if std::env::var("AVENCREW_CONTROL_CRASH_CHILD").as_deref() != Ok("1") {
        return;
    }
    let root = std::path::PathBuf::from(std::env::var("AVENCREW_CONTROL_CRASH_ROOT").unwrap());
    run(async {
        let mut s = LocalStore::open(&root).await.unwrap();
        if std::env::var("AVENCREW_CONTROL_CRASH_OP").as_deref() == Ok("register") {
            register(&mut s).await.unwrap();
        } else {
            accept(&mut s, &command(400, "stop", 1)).await.unwrap();
        }
    });
}
#[test]
fn process_death_before_and_after_commit_recovers_exact_durable_receipt() {
    run(async {
        for op in ["register", "command"] {
            for stage in ["before_commit", "after_commit"] {
                let root = Root::new();
                let mut s = setup(&root).await;
                if op == "command" {
                    register(&mut s).await.unwrap();
                }
                s.close().await.unwrap();
                let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "journal::commands::tests::control_crash_child",
                        "--nocapture",
                    ])
                    .env("AVENCREW_CONTROL_CRASH_CHILD", "1")
                    .env("AVENCREW_CONTROL_CRASH_ROOT", &root.0)
                    .env("AVENCREW_CONTROL_CRASH_OP", op)
                    .env("AVENCREW_CONTROL_CRASH_STAGE", stage)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .unwrap();
                let start = std::time::Instant::now();
                while !root.0.join("control-crash-ready").exists() {
                    if let Some(status) = child.try_wait().unwrap() {
                        panic!("child exited before commit barrier: {status}");
                    }
                    if start.elapsed() > std::time::Duration::from_secs(15) {
                        child.kill().unwrap();
                        child.wait().unwrap();
                        panic!("child timed out");
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
                child.kill().unwrap();
                child.wait().unwrap();
                let mut s = LocalStore::open(&root.0).await.unwrap();
                if op == "register" {
                    assert_eq!(
                        counts(&mut s).await,
                        if stage == "before_commit" {
                            (0, 0, 0)
                        } else {
                            (0, 1, 1)
                        }
                    );
                    let a = register(&mut s).await.unwrap();
                    assert_eq!(register(&mut s).await.unwrap(), a);
                } else {
                    assert_eq!(
                        counts(&mut s).await,
                        if stage == "before_commit" {
                            (0, 1, 1)
                        } else {
                            (1, 2, 2)
                        }
                    );
                    let a = accept(&mut s, &command(400, "stop", 1)).await.unwrap();
                    assert_eq!(accept(&mut s, &command(400, "stop", 1)).await.unwrap(), a);
                    let r = run_row(&mut s.connection, &id(1), &id(300), &id(2))
                        .await
                        .unwrap();
                    assert!(r.cancelled);
                    assert_eq!(r.status, "cancelling");
                }
                s.close().await.unwrap();
            }
        }
    });
}

#[test]
fn actual_sqlite_busy_and_full_refuse_acknowledgement() {
    run(async {
        for full in [false, true] {
            let root = Root::new();
            let mut s = setup(&root).await;
            register(&mut s).await.unwrap();
            let mut other = if full {
                sqlx::query("CREATE TABLE disk_probe(bytes BLOB NOT NULL) STRICT")
                    .execute(&mut s.connection)
                    .await
                    .unwrap();
                let pages: i64 = sqlx::query_scalar("PRAGMA page_count")
                    .fetch_one(&mut s.connection)
                    .await
                    .unwrap();
                sqlx::query(sqlx::AssertSqlSafe(format!(
                    "PRAGMA max_page_count={pages}"
                )))
                .execute(&mut s.connection)
                .await
                .unwrap();
                sqlx::query("CREATE TEMP TRIGGER fill_disk BEFORE INSERT ON local_commands BEGIN INSERT INTO disk_probe VALUES(zeroblob(1048576)); END").execute(&mut s.connection).await.unwrap();
                None
            } else {
                sqlx::query("PRAGMA busy_timeout=50")
                    .execute(&mut s.connection)
                    .await
                    .unwrap();
                let mut c = SqliteConnection::connect_with(
                    &sqlx::sqlite::SqliteConnectOptions::new()
                        .filename(root.0.join("execution.sqlite3")),
                )
                .await
                .unwrap();
                sqlx::query("BEGIN IMMEDIATE")
                    .execute(&mut c)
                    .await
                    .unwrap();
                Some(c)
            };
            let error = accept(&mut s, &command(400, "stop", 1)).await.unwrap_err();
            let ControlError::Store(StoreError::Database(sqlx::Error::Database(error))) = error
            else {
                panic!("wrong failure lane: {error}");
            };
            assert_eq!(error.code().unwrap(), if full { "13" } else { "5" });
            if let Some(c) = &mut other {
                sqlx::query("ROLLBACK").execute(c).await.unwrap();
            }
            assert_eq!(counts(&mut s).await, (0, 1, 1));
            let r = run_row(&mut s.connection, &id(1), &id(300), &id(2))
                .await
                .unwrap();
            assert_eq!((r.version, r.accepted, r.event), (1, 0, 1));
            assert!(!r.cancelled);
            if full {
                sqlx::query("DROP TRIGGER fill_disk")
                    .execute(&mut s.connection)
                    .await
                    .unwrap();
                sqlx::query("DROP TABLE disk_probe")
                    .execute(&mut s.connection)
                    .await
                    .unwrap();
                sqlx::query("PRAGMA max_page_count=4294967294")
                    .execute(&mut s.connection)
                    .await
                    .unwrap();
            }
            accept(&mut s, &command(400, "stop", 1)).await.unwrap();
            s.close().await.unwrap();
        }
    });
}

#[test]
fn recovery_reopens_exact_ordered_controls_without_applying_or_claiming() {
    run(async {
        let root = Root::new();
        let mut s = setup(&root).await;
        register(&mut s).await.unwrap();
        // UUID/key order deliberately differs from acceptance order.
        let first = command(450, "steer", 1);
        let second = command(400, "pause", 2);
        let r1 = accept(&mut s, &first).await.unwrap();
        let r2 = accept(&mut s, &second).await.unwrap();
        let before = counts(&mut s).await;
        s.close().await.unwrap();
        let mut s = LocalStore::open(&root.0).await.unwrap();
        for _ in 0..2 {
            let snapshot = s.recover_local_controls(&id(1), &id(2)).await.unwrap();
            assert_eq!(snapshot.runs.len(), 1);
            let r = &snapshot.runs[0];
            assert_eq!(r.gate, RecoveryGate::AwaitingControlConsumer);
            assert_eq!(
                (r.accepted.value(), r.applied.value(), r.epoch.value()),
                (2, 0, 0)
            );
            assert_eq!(r.pending_controls.len(), 2);
            assert_eq!(r.pending_controls[0].sequence.value(), 1);
            assert_eq!(r.pending_controls[1].sequence.value(), 2);
            assert_eq!(
                serde_json::to_value(&r.pending_controls[0].command).unwrap(),
                first
            );
            assert_eq!(
                serde_json::to_value(&r.pending_controls[1].command).unwrap(),
                second
            );
            assert_eq!(r.pending_controls[0].original_receipt, r1);
            assert_eq!(r.pending_controls[1].original_receipt, r2);
            assert_eq!(counts(&mut s).await, before);
            assert_eq!(
                run_row(&mut s.connection, &id(1), &id(300), &id(2))
                    .await
                    .unwrap()
                    .status,
                "queued"
            );
        }
        accept(&mut s, &command(460, "stop", 3)).await.unwrap();
        let before = counts(&mut s).await;
        let snapshot = s.recover_local_controls(&id(1), &id(2)).await.unwrap();
        assert_eq!(snapshot.runs[0].gate, RecoveryGate::CancellationPending);
        assert_eq!(snapshot.runs[0].pending_controls.len(), 3);
        assert_eq!(snapshot.runs[0].applied.value(), 0);
        assert_eq!(counts(&mut s).await, before);
        s.close().await.unwrap();
    });
}

#[test]
fn recovery_holds_terminal_authority_and_execution_states_without_revival() {
    run(async {
        for (status, authority, epoch, expected) in [
            ("completed", "local", 0, RecoveryGate::Terminal),
            ("cancelled", "local", 0, RecoveryGate::Terminal),
            ("failed", "local", 0, RecoveryGate::Terminal),
            ("queued", "sealed", 0, RecoveryGate::AuthorityHeld),
            ("queued", "transfer_staging", 0, RecoveryGate::AuthorityHeld),
            ("waiting", "local", 0, RecoveryGate::ReconciliationRequired),
            ("running", "local", 2, RecoveryGate::ReconciliationRequired),
            ("blocked", "local", 0, RecoveryGate::ReconciliationRequired),
            ("paused", "local", 0, RecoveryGate::AwaitingControlConsumer),
        ] {
            let root = Root::new();
            let mut s = setup(&root).await;
            register(&mut s).await.unwrap();
            let guard: String = sqlx::query_scalar(
                "SELECT sql FROM sqlite_schema WHERE name='local_runs_state_guard'",
            )
            .fetch_one(&mut s.connection)
            .await
            .unwrap();
            sqlx::query("DROP TRIGGER local_runs_state_guard")
                .execute(&mut s.connection)
                .await
                .unwrap();
            sqlx::query("UPDATE local_runs SET status=?,authority=?,fencing_epoch=? WHERE id=?")
                .bind(status)
                .bind(authority)
                .bind(epoch)
                .bind(id(300).as_str())
                .execute(&mut s.connection)
                .await
                .unwrap();
            sqlx::query(sqlx::AssertSqlSafe(guard.as_str()))
                .execute(&mut s.connection)
                .await
                .unwrap();
            let before = counts(&mut s).await;
            let snapshot = s.recover_local_controls(&id(1), &id(2)).await.unwrap();
            assert_eq!(snapshot.runs[0].gate, expected);
            assert_eq!(counts(&mut s).await, before);
            let row = run_row(&mut s.connection, &id(1), &id(300), &id(2))
                .await
                .unwrap();
            assert_eq!(
                (row.status.as_str(), row.authority.as_str(), row.epoch),
                (status, authority, epoch)
            );
            s.close().await.unwrap();
        }
        // Retained attempt and uninterpreted effect history cannot grant restart.
        for attempt in [true, false] {
            let root = Root::new();
            let mut s = setup(&root).await;
            register(&mut s).await.unwrap();
            if attempt {
                sqlx::query("INSERT INTO local_attempts (id,workspace_id,created_at_us,run_id,epoch,placement,state,harness_build_digest,protocol_version,boot_id,started_at_us) VALUES (?,?,0,?,1,'local','lost',?,1,'previous-boot',0)")
                    .bind(id(610).as_str()).bind(id(1).as_str()).bind(id(300).as_str()).bind("a".repeat(64)).execute(&mut s.connection).await.unwrap();
            } else {
                sqlx::query("INSERT INTO local_events (id,workspace_id,created_at_us,run_id,seq,epoch,event_type,schema_version,occurred_at_us,payload_blob_id) VALUES (?,?,0,?,2,0,'tool.unknown',1,0,?)")
                    .bind(id(610).as_str()).bind(id(1).as_str()).bind(id(300).as_str()).bind(id(303).as_str()).execute(&mut s.connection).await.unwrap();
                sqlx::query("UPDATE local_runs SET last_event_seq=2,row_version=2")
                    .execute(&mut s.connection)
                    .await
                    .unwrap();
            }
            let before = counts(&mut s).await;
            assert_eq!(
                s.recover_local_controls(&id(1), &id(2)).await.unwrap().runs[0].gate,
                RecoveryGate::ReconciliationRequired
            );
            assert_eq!(counts(&mut s).await, before);
            s.close().await.unwrap();
        }
        // A durable wake intent is held, never interpreted as a timer firing.
        let root = Root::new();
        let mut s = setup(&root).await;
        register(&mut s).await.unwrap();
        sqlx::query("INSERT INTO local_work_queue VALUES (?,?,0,?,'wake','wait-test',0,'claimed','old-claim',100,1,?,NULL)").bind(id(600).as_str()).bind(id(1).as_str()).bind(id(300).as_str()).bind(id(303).as_str()).execute(&mut s.connection).await.unwrap();
        assert_eq!(
            s.recover_local_controls(&id(1), &id(2)).await.unwrap().runs[0].gate,
            RecoveryGate::ReconciliationRequired
        );
        let token: String =
            sqlx::query_scalar("SELECT claim_token FROM local_work_queue WHERE id=?")
                .bind(id(600).as_str())
                .fetch_one(&mut s.connection)
                .await
                .unwrap();
        assert_eq!(token, "old-claim");
        s.close().await.unwrap();
    });
}

#[test]
fn recovery_rejects_corrupt_bytes_gaps_cursors_and_revoked_identity() {
    run(async {
        for mode in 0..6 {
            let root = Root::new();
            let mut s = setup(&root).await;
            register(&mut s).await.unwrap();
            accept(&mut s, &command(400, "steer", 1)).await.unwrap();
            accept(&mut s, &command(401, "pause", 2)).await.unwrap();
            match mode {
                0 => {
                    sqlx::query("UPDATE local_commands SET seq=3 WHERE id=?")
                        .bind(id(401).as_str())
                        .execute(&mut s.connection)
                        .await
                        .unwrap();
                }
                1 => {
                    sqlx::query("DELETE FROM local_events WHERE seq=2")
                        .execute(&mut s.connection)
                        .await
                        .unwrap();
                }
                2 => {
                    sqlx::query("UPDATE local_runs SET accepted_command_seq=3")
                        .execute(&mut s.connection)
                        .await
                        .unwrap();
                }
                3 => {
                    sqlx::query("UPDATE local_runs SET applied_command_seq=1")
                        .execute(&mut s.connection)
                        .await
                        .unwrap();
                }
                4 => {
                    std::fs::write(root.0.join(relative(&id(1), &id(30400))), b"corrupt").unwrap();
                }
                _ => {
                    sqlx::query("UPDATE local_actors SET status='disabled'")
                        .execute(&mut s.connection)
                        .await
                        .unwrap();
                }
            }
            let before = counts(&mut s).await;
            assert!(s.recover_local_controls(&id(1), &id(2)).await.is_err());
            assert_eq!(counts(&mut s).await, before);
            s.close().await.unwrap();
        }
    });
}

#[test]
fn retained_runs_are_recovered_in_bounded_pages_without_lifetime_exhaustion() {
    run(async {
        let root = Root::new();
        let mut s = setup(&root).await;
        for n in 0..257 {
            s.register_local_run(LocalRunRegistration {
                workspace: &id(1),
                actor: &id(2),
                task: &id(100),
                task_revision: &id(103),
                run: &id(1000 + n),
                event: &id(3000 + n),
                queue: &id(5000 + n),
                payload_blob: &id(7000 + n),
                at: &at(),
            })
            .await
            .unwrap();
        }
        let page = s.recover_local_controls(&id(1), &id(2)).await.unwrap();
        assert_eq!(page.runs.len(), 256);
        let page = s
            .recover_local_controls_page(&id(1), &id(2), page.next_after.as_ref())
            .await
            .unwrap();
        assert_eq!(page.runs.len(), 1);
        assert_eq!(page.runs[0].run, id(1256));
        assert!(page.next_after.is_none());
        s.close().await.unwrap();
    });
}

#[test]
fn oversized_control_history_holds_only_its_run_and_stop_remains_accepted() {
    run(async {
        let root = Root::new();
        let mut s = setup(&root).await;
        register(&mut s).await.unwrap();
        for n in 0..1025 {
            accept(&mut s, &command(60000 + n, "steer", n + 1))
                .await
                .unwrap();
        }
        s.register_local_run(LocalRunRegistration {
            workspace: &id(1),
            actor: &id(2),
            task: &id(100),
            task_revision: &id(103),
            run: &id(400),
            event: &id(401),
            queue: &id(402),
            payload_blob: &id(403),
            at: &at(),
        })
        .await
        .unwrap();
        let page = s.recover_local_controls(&id(1), &id(2)).await.unwrap();
        assert_eq!(page.runs[0].gate, RecoveryGate::HistoryLimit);
        assert!(!page.runs[0].history_complete);
        assert!(page.runs[0].pending_controls.is_empty());
        assert_eq!(page.runs[1].gate, RecoveryGate::AwaitingControlConsumer);
        assert!(page.runs[1].history_complete);
        accept(&mut s, &command(62000, "stop", 1026)).await.unwrap();
        let page = s.recover_local_controls(&id(1), &id(2)).await.unwrap();
        assert_eq!(page.runs[0].gate, RecoveryGate::CancellationPending);
        assert!(!page.runs[0].history_complete);
        s.close().await.unwrap();
    });
}
