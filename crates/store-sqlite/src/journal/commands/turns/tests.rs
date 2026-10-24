use super::super::tests::{register, setup};
use super::*;
use crate::journal::tests::{run, Root};
use serde_json::json;
fn id(n: u32) -> DomainId {
    DomainId::new(format!("01900000-0000-7000-8000-{n:012x}")).unwrap()
}
fn at() -> Instant {
    Instant::new("2026-10-11T00:00:00.000000Z").unwrap()
}
pub(crate) fn seed() -> RpcRequestHarnessStartTurnBody {
    serde_json::from_value(json!({
    "run_spec":{"task_id":id(100),"task_revision_id":id(103),"run_id":id(300),"session_id":id(701),"branch_id":id(702),"workspace_id":id(1),"project_id":null,"worker_id":null,"responsibility_id":null,"requester_id":id(3),"owner_id":id(3),"acting_principal_id":id(3),"audience_policy_id":id(6),"execution_policy_id":id(7),"objective_version_id":id(102),"check_refs":[id(104)],"instruction_digests":[],"model_profile_id":id(750),"environment_recipe_version_id":null,"budget_period_refs":[],"deadline":null,"placement":"local","accepted_input_seq":"0"},
    "lease":{"run_id":id(300),"attempt_id":id(703),"epoch":"1","placement":"local","authority":"local","environment_id":null,"generation":null,"expires_at":"2026-10-11T01:00:00.000000Z","boot_id":id(704),"monotonic_deadline_ns":"1000","receipt_digest":"0".repeat(64)},
    "turn_id":id(700),"accepted_input_cut":"0","checkpoint_id":null,"limits":{"model_steps":5,"active_wall_ms":1000,"output_bytes":32768}
})).unwrap()
}
pub(crate) async fn publish(
    s: &mut LocalStore,
    seed: &RpcRequestHarnessStartTurnBody,
) -> Result<LocalTurnReceipt, TurnError> {
    s.publish_local_turn(LocalTurnPublication {
        actor: &id(2),
        seed,
        ordinal: &PositiveCounter::new("1").unwrap(),
        expected_run_version: &PositiveCounter::new("1").unwrap(),
        spec_blob: &id(705),
        lease_blob: &id(706),
        event: &id(707),
        harness_build_digest: &Digest::new("1".repeat(64)).unwrap(),
        at: &at(),
        now: &at(),
        current_boot: &id(704),
        monotonic_now: &Counter::new("10").unwrap(),
    })
    .await
}
pub(super) fn barrier(database: &std::path::Path, stage: &str) -> Result<(), TurnError> {
    if std::env::var("AVENCREW_TURN_CRASH_CHILD").as_deref() != Ok("1")
        || std::env::var("AVENCREW_TURN_CRASH_STAGE").as_deref() != Ok(stage)
    {
        return Ok(());
    }
    let root = database.parent().unwrap();
    std::fs::write(root.join("turn-crash-ready"), stage).map_err(StoreError::from)?;
    let started = std::time::Instant::now();
    while started.elapsed() < std::time::Duration::from_secs(30) {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    Err(TurnError::Conflict)
}
#[test]
fn original_seed_receipt_and_fenced_projection_survive_reopen() {
    run(async {
        let root = Root::new();
        let mut s = setup(&root).await;
        register(&mut s).await.unwrap();
        let receipt = publish(&mut s, &seed()).await.unwrap();
        assert_eq!(receipt.run_version.value(), 2);
        assert_eq!(receipt.event_sequence.value(), 2);
        assert_eq!(publish(&mut s, &seed()).await.unwrap(), receipt);
        assert!(sqlx::query("UPDATE local_turns SET input_command_seq=1")
            .execute(&mut s.connection)
            .await
            .is_err());
        assert!(sqlx::query("UPDATE local_events SET occurred_at_us=occurred_at_us+1 WHERE event_type='turn.prepared'").execute(&mut s.connection).await.is_err());
        let report = s.diagnostics().await.unwrap();
        assert_eq!(report.counts.turns.value(), 1);
        assert!(!report.execution_enabled);
        s.close().await.unwrap();
        let mut s = LocalStore::open(&root.0).await.unwrap();
        let restored = s.read_local_turn(&id(1), &id(2), &id(700)).await.unwrap();
        assert_eq!(restored.seed, seed());
        assert_eq!(restored.receipt, receipt);
        assert_eq!(publish(&mut s, &seed()).await.unwrap(), receipt);
        let snap = s.recover_local_controls(&id(1), &id(2)).await.unwrap();
        assert_eq!(snap.runs[0].gate, RecoveryGate::ReconciliationRequired);
        let row = run_row(&mut s.connection, &id(1), &id(300), &id(2))
            .await
            .unwrap();
        assert_eq!(row.epoch, 1);
        assert_eq!(row.status, "preparing");
        s.close().await.unwrap();
    });
}
#[test]
fn changed_seed_scope_clock_control_and_identity_fail_without_turns() {
    run(async {
        for case in 0..11 {
            let root = Root::new();
            let mut s = setup(&root).await;
            register(&mut s).await.unwrap();
            let mut request = seed();
            match case {
                0 => request.run_spec.acting_principal_id = id(99),
                1 => request.run_spec.objective_version_id = id(99),
                2 => request.lease.epoch = PositiveCounter::new("2").unwrap(),
                3 => request.lease.boot_id = id(99),
                4 => request.lease.monotonic_deadline_ns = Counter::new("10").unwrap(),
                5 => request.lease.expires_at = at(),
                6 => request.accepted_input_cut = Counter::new("1").unwrap(),
                7 => request.run_spec.project_id = Nullable(Some(id(99))),
                8 => request.run_spec.placement = Placement::Cloud,
                9 => {
                    sqlx::query(
                        "UPDATE local_runs SET status='cancelling',cancellation_requested=1",
                    )
                    .execute(&mut s.connection)
                    .await
                    .unwrap();
                }
                _ => {
                    sqlx::query("UPDATE local_runs SET authority='sealed'")
                        .execute(&mut s.connection)
                        .await
                        .unwrap();
                }
            }
            assert!(publish(&mut s, &request).await.is_err(), "case {case}");
            assert_eq!(s.diagnostics().await.unwrap().counts.turns.value(), 0);
            s.close().await.unwrap();
        }
        let root = Root::new();
        let mut s = setup(&root).await;
        register(&mut s).await.unwrap();
        let receipt = publish(&mut s, &seed()).await.unwrap();
        let mut changed = seed();
        changed.limits.model_steps = avencrew_contracts::scalars::SmallInteger::new(4).unwrap();
        assert!(matches!(
            publish(&mut s, &changed).await,
            Err(TurnError::IdempotencyMismatch)
        ));
        assert_eq!(
            s.read_local_turn(&id(1), &id(2), &id(700))
                .await
                .unwrap()
                .receipt,
            receipt
        );
        assert!(s.read_local_turn(&id(1), &id(99), &id(700)).await.is_err());
        s.close().await.unwrap();
    });
}
#[test]
fn transaction_rollback_and_corrupt_payload_never_acknowledge_turn() {
    run(async {
        let root = Root::new();
        let mut s = setup(&root).await;
        register(&mut s).await.unwrap();
        sqlx::query("CREATE TRIGGER turn_cut BEFORE INSERT ON local_turns BEGIN SELECT RAISE(ABORT,'cut'); END").execute(&mut s.connection).await.unwrap();
        assert!(publish(&mut s, &seed()).await.is_err());
        for query in [
            "SELECT count(*) FROM local_turns",
            "SELECT count(*) FROM local_attempts",
            "SELECT count(*) FROM local_sessions",
            "SELECT count(*) FROM local_session_branches",
        ] {
            let n: i64 = sqlx::query_scalar(query)
                .fetch_one(&mut s.connection)
                .await
                .unwrap();
            assert_eq!(n, 0);
        }
        assert_eq!(
            run_row(&mut s.connection, &id(1), &id(300), &id(2))
                .await
                .unwrap()
                .epoch,
            0
        );
        sqlx::query("DROP TRIGGER turn_cut")
            .execute(&mut s.connection)
            .await
            .unwrap();
        publish(&mut s, &seed()).await.unwrap();
        std::fs::write(root.0.join(relative(&id(1), &id(705))), b"corrupt").unwrap();
        assert!(s.read_local_turn(&id(1), &id(2), &id(700)).await.is_err());
        assert!(publish(&mut s, &seed()).await.is_err());
        s.close().await.unwrap();
    });
}
#[test]
fn turn_crash_child() {
    if std::env::var("AVENCREW_TURN_CRASH_CHILD").as_deref() != Ok("1") {
        return;
    }
    run(async {
        let path = std::path::PathBuf::from(std::env::var("AVENCREW_TURN_CRASH_ROOT").unwrap());
        let mut s = LocalStore::open(&path).await.unwrap();
        publish(&mut s, &seed()).await.unwrap();
    });
}
#[test]
fn actual_child_kill_keeps_only_acknowledged_atomic_publication() {
    for stage in ["before_commit", "after_commit"] {
        let root = Root::new();
        run(async {
            let mut s = setup(&root).await;
            register(&mut s).await.unwrap();
            s.close().await.unwrap();
        });
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "journal::commands::turns::tests::turn_crash_child",
                "--nocapture",
            ])
            .env("AVENCREW_TURN_CRASH_CHILD", "1")
            .env("AVENCREW_TURN_CRASH_STAGE", stage)
            .env("AVENCREW_TURN_CRASH_ROOT", &root.0)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let started = std::time::Instant::now();
        while !root.0.join("turn-crash-ready").exists() {
            if child.try_wait().unwrap().is_some() {
                panic!("child exited before {stage}");
            }
            assert!(started.elapsed() < std::time::Duration::from_secs(15));
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        child.kill().unwrap();
        child.wait().unwrap();
        run(async {
            let mut s = LocalStore::open(&root.0).await.unwrap();
            let rows = s.diagnostics().await.unwrap().counts.turns.value();
            assert_eq!(rows, u64::from(stage == "after_commit"));
            if stage == "after_commit" {
                assert_eq!(
                    s.read_local_turn(&id(1), &id(2), &id(700))
                        .await
                        .unwrap()
                        .seed,
                    seed()
                );
            } else {
                assert!(s.read_local_turn(&id(1), &id(2), &id(700)).await.is_err());
                publish(&mut s, &seed()).await.unwrap();
            }
            s.close().await.unwrap();
        });
    }
}

#[test]
fn historical_retry_never_renews_after_stop_expiry_or_boot_change() {
    run(async {
        let root = Root::new();
        let mut s = setup(&root).await;
        register(&mut s).await.unwrap();
        let receipt = publish(&mut s, &seed()).await.unwrap();
        let command=serde_json::to_vec(&json!({"kind":"stop","command_id":id(800),"run_id":id(300),"idempotency_key":"stop-after-preparation","payload":{"reason":"stop"}})).unwrap();
        s.accept_command(CommandAcceptance {
            workspace: &id(1),
            actor: &id(2),
            bytes: &command,
            event: &id(801),
            queue: &id(802),
            command_blob: &id(803),
            event_blob: &id(804),
            at: &at(),
        })
        .await
        .unwrap();
        let original = seed();
        let retried = s
            .publish_local_turn(LocalTurnPublication {
                actor: &id(2),
                seed: &original,
                ordinal: &PositiveCounter::new("1").unwrap(),
                expected_run_version: &PositiveCounter::new("1").unwrap(),
                spec_blob: &id(705),
                lease_blob: &id(706),
                event: &id(707),
                harness_build_digest: &Digest::new("1".repeat(64)).unwrap(),
                at: &at(),
                now: &Instant::new("2026-10-11T02:00:00.000000Z").unwrap(),
                current_boot: &id(999),
                monotonic_now: &Counter::new("10000").unwrap(),
            })
            .await
            .unwrap();
        assert_eq!(retried, receipt);
        let row = run_row(&mut s.connection, &id(1), &id(300), &id(2))
            .await
            .unwrap();
        assert_eq!(row.status, "cancelling");
        assert!(row.cancelled);
        assert_eq!(row.epoch, 1);
        assert_eq!(
            s.read_local_turn(&id(1), &id(2), &id(700))
                .await
                .unwrap()
                .seed,
            original
        );
        let report = s.diagnostics().await.unwrap();
        assert_eq!(report.counts.turns.value(), 1);
        assert_eq!(report.counts.attempts.value(), 1);
        assert!(!report.execution_enabled);
        s.close().await.unwrap();
    });
}
#[test]
fn projection_mismatch_and_live_erasure_deny_retained_seed_release() {
    run(async {
        for mutation in [
            "UPDATE local_attempts SET placement='cloud'",
            "UPDATE local_attempts SET monotonic_deadline_ns=monotonic_deadline_ns+1",
            "UPDATE local_actors SET status='erased'",
        ] {
            let root = Root::new();
            let mut s = setup(&root).await;
            register(&mut s).await.unwrap();
            publish(&mut s, &seed()).await.unwrap();
            sqlx::query(mutation)
                .execute(&mut s.connection)
                .await
                .unwrap();
            assert!(s.read_local_turn(&id(1), &id(2), &id(700)).await.is_err());
            assert!(publish(&mut s, &seed()).await.is_err());
            s.close().await.unwrap();
        }
    });
}
