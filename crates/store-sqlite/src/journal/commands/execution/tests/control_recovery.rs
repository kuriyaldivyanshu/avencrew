use super::*;
use avencrew_contracts::{
    scalars::List,
    wire::{CommandReceipt, TurnOutcome},
};

pub(super) async fn command(s: &mut LocalStore, kind: &str, n: u32) -> (Vec<u8>, CommandReceipt) {
    command_for_run(s, kind, n, &id(300)).await
}
async fn command_for_run(
    s: &mut LocalStore,
    kind: &str,
    n: u32,
    run: &DomainId,
) -> (Vec<u8>, CommandReceipt) {
    let n = n + 10_000; // Separate from the canonical task's allocated blob IDs.
    let row = run_row(&mut s.connection, &id(1), run, &id(2))
        .await
        .unwrap();
    let payload = match kind {
        "stop" => json!({"reason":"Stop"}),
        "pause" => json!({"reason":"Pause","expected_revision":row.version.to_string()}),
        _ => json!({"text":"New input","expected_revision":row.version.to_string()}),
    };
    let bytes=serde_json::to_vec(&json!({"kind":kind,"command_id":id(n),"run_id":run,"idempotency_key":format!("control-{n}"),"payload":payload})).unwrap();
    let receipt = s
        .accept_command(CommandAcceptance {
            workspace: &id(1),
            actor: &id(2),
            bytes: &bytes,
            event: &id(n + 1),
            queue: &id(n + 2),
            command_blob: &id(n + 3),
            event_blob: &id(n + 4),
            at: &now(),
        })
        .await
        .unwrap();
    (bytes, receipt)
}
async fn retry(s: &mut LocalStore, bytes: &[u8]) -> CommandReceipt {
    s.accept_command(CommandAcceptance {
        workspace: &id(1),
        actor: &id(2),
        bytes,
        event: &id(4990),
        queue: &id(4991),
        command_blob: &id(4992),
        event_blob: &id(4993),
        at: &now(),
    })
    .await
    .unwrap()
}
async fn counts(s: &mut LocalStore) -> (i64, i64, i64, i64) {
    sqlx::query_as("SELECT (SELECT count(*) FROM local_model_calls),(SELECT count(*) FROM local_dispatches),(SELECT count(*) FROM local_events),(SELECT row_version FROM local_runs)").fetch_one(&mut s.connection).await.unwrap()
}
#[test]
fn ordered_controls_reopen_with_exact_dispositions_original_receipts_and_no_writes() {
    native_run(|| async {
        let root = Root::new();
        let mut s = ready(&root).await;
        // IDs deliberately run backwards; acceptance order owns the cursor.
        let (steer, a) = command(&mut s, "steer", 1200).await;
        let (pause, b) = command(&mut s, "pause", 1100).await;
        let (stop, c) = command(&mut s, "stop", 1000).await;
        let applied = s.apply_local_controls(access(), &id(1300)).await.unwrap();
        assert!(applied.stopped);
        assert_eq!(applied.applied_input_cut.value(), 3);
        let before = counts(&mut s).await;
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT kind,state FROM local_commands ORDER BY seq")
                .fetch_all(&mut s.connection)
                .await
                .unwrap();
        assert_eq!(
            rows,
            vec![
                ("steer".into(), "rejected".into()),
                ("pause".into(), "rejected".into()),
                ("cancel".into(), "applied".into())
            ]
        );
        s.close().await.unwrap();
        let mut s = LocalStore::open(&root.0).await.unwrap();
        assert_eq!(retry(&mut s, &steer).await, a);
        assert_eq!(retry(&mut s, &pause).await, b);
        assert_eq!(retry(&mut s, &stop).await, c);
        let snap = s
            .reconcile_local_execution(&id(1), &id(2), &id(700))
            .await
            .unwrap();
        assert_eq!(snap.progress.state.applied_input_cut.value(), 3);
        s.apply_local_controls(access(), &id(1301)).await.unwrap();
        let restored = s.recover_local_controls(&id(1), &id(2)).await.unwrap();
        assert_eq!(
            restored.runs[0].gate,
            super::super::super::recovery::RecoveryGate::CancellationPending
        );
        assert!(restored.runs[0].pending_controls.is_empty());
        assert_eq!(counts(&mut s).await, before);
        s.close().await.unwrap();
    });
}
#[test]
fn historical_steering_is_not_superseded_by_a_later_accepted_stop() {
    native_run(|| async {
        let root = Root::new();
        let mut s = ready(&root).await;
        command(&mut s, "steer", 1000).await;
        s.apply_local_controls(access(), &id(1300)).await.unwrap();
        command(&mut s, "stop", 1100).await;
        s.apply_local_controls(access(), &id(1301)).await.unwrap();
        s.close().await.unwrap();
        let mut s = LocalStore::open(&root.0).await.unwrap();
        s.reconcile_local_execution(&id(1), &id(2), &id(700))
            .await
            .unwrap();
        let states: Vec<String> =
            sqlx::query_scalar("SELECT state FROM local_commands ORDER BY seq")
                .fetch_all(&mut s.connection)
                .await
                .unwrap();
        assert_eq!(states, vec!["applied", "applied"]);
        s.close().await.unwrap();
    });
}
#[test]
fn stop_and_real_identity_revocation_after_exposure_deny_fresh_dispatch() {
    native_run(|| async {
        for change in ["stop", "actor", "device", "permission", "epoch", "sealed"] {
            let root = Root::new();
            stage(&root, "dispatch_before_commit").await;
            let mut s = LocalStore::open(&root.0).await.unwrap();
            assert_eq!(
                s.read_local_proposal(&id(1), &id(2), &id(820))
                    .await
                    .unwrap()
                    .invocation_id,
                id(820)
            );
            match change {
                "stop" => {
                    command(&mut s, "stop", 1000).await;
                }
                "actor" => {
                    sqlx::query("UPDATE local_actors SET status='disabled'")
                        .execute(&mut s.connection)
                        .await
                        .unwrap();
                }
                "device" => {
                    sqlx::query("UPDATE local_devices SET status='revoked'")
                        .execute(&mut s.connection)
                        .await
                        .unwrap();
                }
                "permission" => {
                    sqlx::query("UPDATE local_workspaces SET permission_generation=1")
                        .execute(&mut s.connection)
                        .await
                        .unwrap();
                }
                "epoch" => {
                    sqlx::query("UPDATE local_runs SET fencing_epoch=fencing_epoch+1")
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
            assert!(
                s.claim_local_dispatch(
                    access(),
                    &proposal(),
                    &avencrew_contracts::scalars::Digest::new("0".repeat(64)).unwrap(),
                    &id(1400)
                )
                .await
                .is_err(),
                "dispatch after {change}"
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM local_dispatches")
                    .fetch_one(&mut s.connection)
                    .await
                    .unwrap(),
                0
            );
            s.close().await.unwrap();
        }
    });
}
#[test]
fn replay_refuses_disposition_cursor_and_historical_payload_drift_at_startup() {
    native_run(|| async {
        for change in 0..3 {
            let root = Root::new();
            let mut s = ready(&root).await;
            command(&mut s, "steer", 1000).await;
            s.apply_local_controls(access(), &id(1300)).await.unwrap();
            // Leave a newer valid progress event, so old corrupt bytes cannot
            // hide behind a perfectly readable latest runtime pointer.
            command(&mut s, "stop", 1100).await;
            s.apply_local_controls(access(), &id(1301)).await.unwrap();
            match change {
                0 => {
                    sqlx::query("UPDATE local_commands SET state='rejected' WHERE kind='steer'")
                        .execute(&mut s.connection)
                        .await
                        .unwrap();
                }
                1 => {
                    sqlx::query("UPDATE local_runs SET applied_command_seq=1")
                        .execute(&mut s.connection)
                        .await
                        .unwrap();
                }
                _ => {
                    std::fs::write(
                        root.0.join(format!(
                            "blobs/{}/{}.json",
                            id(1).as_str(),
                            id(1300).as_str()
                        )),
                        b"corrupt old disposition",
                    )
                    .unwrap();
                }
            }
            s.close().await.unwrap();
            let mut s = LocalStore::open(&root.0).await.unwrap();
            assert!(s
                .reconcile_local_execution(&id(1), &id(2), &id(700))
                .await
                .is_err());
            assert!(s.recover_local_controls(&id(1), &id(2)).await.is_err());
            s.close().await.unwrap();
        }
    });
}

pub(super) fn cancellation(snap: &LocalExecutionSnapshot, checkpoint: DomainId) -> TurnOutcome {
    let unresolved = snap
        .unresolved_invocations
        .iter()
        .chain(&snap.unresolved_model_calls)
        .cloned()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    TurnOutcome::Cancelled {
        cleanup_refs: List::new(vec![]).unwrap(),
        unresolved_receipt_refs: List::new(unresolved).unwrap(),
        checkpoint_id: Nullable(Some(checkpoint)),
    }
}
pub(super) async fn checkpoint_stop(s: &mut LocalStore) {
    s.apply_local_controls(access(), &id(1300)).await.unwrap();
    let state = s
        .local_execution_snapshot(&id(1), &id(2), &id(700))
        .await
        .unwrap()
        .progress
        .state;
    s.checkpoint_local_execution(LocalExecutionCheckpoint {
        access: access(),
        state: &state,
        checkpoint: &id(1500),
        manifest: &id(1501),
        coverage_entry: &id(1502),
        execution_entry: &id(1503),
        bundle_blob: &id(1504),
        event: &id(1505),
        event_blob: &id(1506),
    })
    .await
    .unwrap();
}
#[test]
fn cancellation_outcome_retains_unresolved_work_reservation_and_never_revives() {
    native_run(|| async {
        for pending in ["none", "model", "tool"] {
            let root = Root::new();
            if pending == "tool" {
                stage(&root, "result_before_commit").await;
            } else {
                ready(&root).await.close().await.unwrap();
            }
            let mut s = LocalStore::open(&root.0).await.unwrap();
            if pending == "model" {
                let mut state = s
                    .local_execution_snapshot(&id(1), &id(2), &id(700))
                    .await
                    .unwrap()
                    .progress
                    .state;
                state.steps_started = 1;
                s.seal_local_step(access(), &tool_step(), &state, &id(810))
                    .await
                    .unwrap();
            }
            let (stop, receipt) = command(&mut s, "stop", 1000).await;
            checkpoint_stop(&mut s).await;
            let snap = s
                .local_execution_snapshot(&id(1), &id(2), &id(700))
                .await
                .unwrap();
            let outcome = cancellation(&snap, id(1500));
            if pending != "none" {
                let empty = TurnOutcome::Cancelled {
                    cleanup_refs: List::new(vec![]).unwrap(),
                    unresolved_receipt_refs: List::new(vec![]).unwrap(),
                    checkpoint_id: Nullable(Some(id(1500))),
                };
                assert!(s
                    .commit_local_outcome(access(), &snap.progress.state, &empty, &id(1600))
                    .await
                    .is_err());
            }
            s.commit_local_outcome(access(), &snap.progress.state, &outcome, &id(1600))
                .await
                .unwrap();
            let before = counts(&mut s).await;
            s.close().await.unwrap();
            let mut s = LocalStore::open(&root.0).await.unwrap();
            assert_eq!(retry(&mut s, &stop).await, receipt);
            let replay = s
                .reconcile_local_execution(&id(1), &id(2), &id(700))
                .await
                .unwrap();
            assert_eq!(replay.progress.outcome, Some(outcome.clone()));
            s.commit_local_outcome(access(), &snap.progress.state, &outcome, &id(1601))
                .await
                .unwrap();
            let (status, cancelled): (String, i64) =
                sqlx::query_as("SELECT status,cancellation_requested FROM local_runs")
                    .fetch_one(&mut s.connection)
                    .await
                    .unwrap();
            let (reservation, ended): (String, Option<i64>) =
                sqlx::query_as("SELECT state,ended_at_us FROM local_attempts")
                    .fetch_one(&mut s.connection)
                    .await
                    .unwrap();
            assert_eq!(cancelled, 1);
            assert_eq!(
                status,
                if pending == "none" {
                    "cancelled"
                } else {
                    "blocked"
                }
            );
            assert_eq!(
                reservation,
                if pending == "none" {
                    "released"
                } else {
                    "draining"
                }
            );
            assert_eq!(ended.is_some(), pending == "none");
            assert!(s.authorize_local_execution(&access(), false).await.is_err());
            assert_eq!(counts(&mut s).await, before);
            if pending != "none" {
                let late = serde_json::to_vec(&json!({"kind":"stop","command_id":id(1700),"run_id":id(300),"idempotency_key":"late-stop","payload":{"reason":"late stop"}})).unwrap();
                assert!(matches!(
                    s.accept_command(CommandAcceptance {
                        workspace: &id(1),
                        actor: &id(2),
                        bytes: &late,
                        event: &id(1701),
                        queue: &id(1702),
                        command_blob: &id(1703),
                        event_blob: &id(1704),
                        at: &now(),
                    })
                    .await,
                    Err(ControlError::Conflict)
                ));
                assert_eq!(counts(&mut s).await, before);
            } else {
                let (_, receipt) = command(&mut s, "stop", 1700).await;
                assert!(matches!(receipt, CommandReceipt::Terminal { .. }));
                assert_eq!(counts(&mut s).await, before);
            }
            s.close().await.unwrap();
        }
    });
}

#[test]
fn real_kills_at_control_application_and_cancellation_outcome_keep_one_atomic_cut() {
    native_run(|| async {
        for name in [
            "controls_before_commit",
            "controls_after_commit",
            "outcome_before_commit",
            "outcome_after_commit",
        ] {
            let root = Root::new();
            let mut s = ready(&root).await;
            let mut state = s
                .local_execution_snapshot(&id(1), &id(2), &id(700))
                .await
                .unwrap()
                .progress
                .state;
            state.steps_started = 1;
            s.seal_local_step(access(), &tool_step(), &state, &id(810))
                .await
                .unwrap();
            let (stop, receipt) = command(&mut s, "stop", 1000).await;
            if name.starts_with("outcome_") {
                checkpoint_stop(&mut s).await;
            }
            s.close().await.unwrap();
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "journal::commands::execution::tests::execution_crash_child",
                    "--nocapture",
                ])
                .env("AVENCREW_EXECUTION_CRASH_ROOT", &root.0)
                .env("AVENCREW_EXECUTION_CRASH_STAGE", name)
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap();
            let started = std::time::Instant::now();
            while !root.0.join("execution-crash-ready").exists() {
                assert!(
                    child.try_wait().unwrap().is_none(),
                    "child exited before {name}"
                );
                if started.elapsed() > std::time::Duration::from_secs(10) {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("barrier unavailable {name}");
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            child.kill().unwrap();
            child.wait().unwrap();
            let mut s = LocalStore::open(&root.0).await.unwrap();
            assert_eq!(retry(&mut s, &stop).await, receipt);
            let snap = s
                .reconcile_local_execution(&id(1), &id(2), &id(700))
                .await
                .unwrap();
            let after = name.ends_with("after_commit");
            if name.starts_with("controls_") {
                assert_eq!(
                    snap.progress.state.applied_input_cut.value(),
                    u64::from(after)
                );
                let before = counts(&mut s).await;
                s.apply_local_controls(access(), &id(1300)).await.unwrap();
                if after {
                    assert_eq!(counts(&mut s).await, before);
                }
            } else {
                assert_eq!(snap.progress.outcome.is_some(), after);
                let before = counts(&mut s).await;
                let outcome = cancellation(&snap, id(1500));
                s.commit_local_outcome(access(), &snap.progress.state, &outcome, &id(1600))
                    .await
                    .unwrap();
                if after {
                    assert_eq!(counts(&mut s).await, before);
                }
                let row:(String,String,Option<i64>)=sqlx::query_as("SELECT r.status,a.state,a.ended_at_us FROM local_runs r JOIN local_attempts a ON a.workspace_id=r.workspace_id AND a.run_id=r.id").fetch_one(&mut s.connection).await.unwrap();
                assert_eq!(row, ("blocked".into(), "draining".into(), None));
                assert!(s.authorize_local_execution(&access(), false).await.is_err());
            }
            s.close().await.unwrap();
        }
    });
}

#[test]
fn unsupported_history_and_sealed_authority_stay_held_without_startup_dispatch() {
    native_run(|| async {
        for sealed in [false, true] {
            let root = Root::new();
            let mut s = ready(&root).await;
            if sealed {
                sqlx::query("UPDATE local_runs SET authority='sealed'")
                    .execute(&mut s.connection)
                    .await
                    .unwrap();
            } else {
                let bytes = canonical(&json!({"state":"unresolved future effect"})).unwrap();
                s.put_canonical(&id(1), &id(1900), &bytes).unwrap();
                let a = access();
                let mut tx = s.connection.begin().await.unwrap();
                blob(
                    &mut tx,
                    &id(1),
                    &id(2),
                    &id(1900),
                    &bytes,
                    "application/vnd.avencrew.execution-observation+json",
                    now().unix_micros(),
                )
                .await
                .unwrap();
                append_execution_event(
                    &mut tx,
                    &a,
                    &seed(),
                    &id(1900),
                    "effect.unavailable",
                    &id(1900),
                )
                .await
                .unwrap();
                tx.commit().await.unwrap();
            }
            let before = counts(&mut s).await;
            s.close().await.unwrap();
            let mut s = LocalStore::open(&root.0).await.unwrap();
            assert!(matches!(
                s.reconcile_local_execution(&id(1), &id(2), &id(700)).await,
                Err(TurnError::StaleAuthority | TurnError::Unsupported)
            ));
            let recovered = s.recover_local_controls(&id(1), &id(2)).await.unwrap();
            assert_eq!(
                recovered.runs[0].gate,
                if sealed {
                    super::super::super::recovery::RecoveryGate::AuthorityHeld
                } else {
                    super::super::super::recovery::RecoveryGate::ReconciliationRequired
                }
            );
            assert_eq!(counts(&mut s).await, before);
            s.close().await.unwrap();
        }
    });
}
#[test]
fn ignored_application_updates_roll_back_dispositions_cursor_and_progress() {
    native_run(|| async {
        for table in ["local_commands", "local_runs"] {
            let root = Root::new();
            let mut s = ready(&root).await;
            command(&mut s, "stop", 1000).await;
            let before = counts(&mut s).await;
            let sql = if table == "local_commands" {
                "CREATE TEMP TRIGGER ignore_application BEFORE UPDATE OF state ON local_commands BEGIN SELECT RAISE(IGNORE); END"
            } else {
                "CREATE TEMP TRIGGER ignore_application BEFORE UPDATE OF applied_command_seq ON local_runs BEGIN SELECT RAISE(IGNORE); END"
            };
            sqlx::query(sql).execute(&mut s.connection).await.unwrap();
            assert!(s.apply_local_controls(access(), &id(1300)).await.is_err());
            assert_eq!(counts(&mut s).await, before);
            assert_eq!(
                sqlx::query_scalar::<_, String>("SELECT state FROM local_commands")
                    .fetch_one(&mut s.connection)
                    .await
                    .unwrap(),
                "accepted"
            );
            assert_eq!(
                s.local_execution_snapshot(&id(1), &id(2), &id(700))
                    .await
                    .unwrap()
                    .progress
                    .state
                    .applied_input_cut
                    .value(),
                0
            );
            s.close().await.unwrap();
        }
    });
}

#[test]
fn execution_replay_budget_is_shared_by_the_page_and_holds_unread_history() {
    native_run(|| async {
        let root = Root::new();
        let mut s = ready(&root).await;
        command(&mut s, "steer", 1000).await;
        s.apply_local_controls(access(), &id(1300)).await.unwrap();
        // A second real prepared run, with its own immutable seed and controls.
        s.register_local_run(LocalRunRegistration {
            workspace: &id(1),
            actor: &id(2),
            task: &id(100),
            task_revision: &id(103),
            run: &id(2000),
            event: &id(2001),
            queue: &id(2002),
            payload_blob: &id(2003),
            at: &now(),
        })
        .await
        .unwrap();
        let mut second = seed();
        second.run_spec.run_id = id(2000);
        second.run_spec.session_id = id(2101);
        second.run_spec.branch_id = id(2102);
        second.lease.run_id = id(2000);
        second.lease.attempt_id = id(2103);
        second.turn_id = id(2100);
        s.publish_local_turn(LocalTurnPublication {
            actor: &id(2),
            seed: &second,
            ordinal: &positive(1).unwrap(),
            expected_run_version: &positive(1).unwrap(),
            spec_blob: &id(2105),
            lease_blob: &id(2106),
            event: &id(2107),
            harness_build_digest: &avencrew_contracts::scalars::Digest::new("1".repeat(64))
                .unwrap(),
            at: &now(),
            now: &now(),
            current_boot: &id(704),
            monotonic_now: &Counter::new("10").unwrap(),
        })
        .await
        .unwrap();
        command_for_run(&mut s, "steer", 2000, &id(2000)).await;
        s.apply_local_controls(
            LocalExecutionAccess::new(
                &id(1),
                &id(2),
                &id(2100),
                &now(),
                &id(704),
                &Counter::new("10").unwrap(),
            ),
            &id(2300),
        )
        .await
        .unwrap();
        let charge: i64 = sqlx::query_scalar("SELECT sum(byte_size) FROM local_blobs WHERE id IN (SELECT payload_blob_id FROM local_commands WHERE run_id=? UNION ALL SELECT payload_blob_id FROM local_events WHERE run_id=? AND event_type='execution.controls_applied')")
            .bind(id(300).as_str()).bind(id(300).as_str()).fetch_one(&mut s.connection).await.unwrap();
        let before = counts(&mut s).await;
        // Corrupt only the second progress file: a budget-held page must never
        // open it. A later explicit, sufficiently budgeted replay detects it.
        let path = root.0.join(relative(&id(1), &id(2300)));
        let bytes = std::fs::read(&path).unwrap();
        std::fs::write(&path, b"unread corrupt progress").unwrap();
        let page = s
            .recover_local_controls_page_with_replay_budget(&id(1), &id(2), None, charge as usize)
            .await
            .unwrap();
        assert!(page.runs[0].history_complete);
        assert!(!page.runs[1].history_complete);
        assert_eq!(
            page.runs[1].gate,
            super::super::super::recovery::RecoveryGate::HistoryLimit
        );
        assert!(s.recover_local_controls(&id(1), &id(2)).await.is_err());
        std::fs::write(&path, bytes).unwrap();
        let page = s.recover_local_controls(&id(1), &id(2)).await.unwrap();
        assert!(page.runs.iter().all(|r| r.history_complete));
        assert_eq!(counts(&mut s).await, before);
        s.close().await.unwrap();
    });
}

#[test]
fn observed_invocation_ids_are_verified_against_the_retained_step_and_rows() {
    native_run(|| async {
        for invalid in [vec![id(999)], vec![id(820), id(820)]] {
            let root = Root::new();
            stage(&root, "result_after_commit").await;
            let mut s = LocalStore::open(&root.0).await.unwrap();
            s.record_local_tool_result(access(), &proposal(), Some(&tool_result()), &id(816))
                .await
                .unwrap();
            let state = s
                .local_execution_snapshot(&id(1), &id(2), &id(700))
                .await
                .unwrap()
                .progress
                .state;
            s.observe_local_step(access(), &id(800), &[id(820)], &state, &id(1800))
                .await
                .unwrap();
            s.reconcile_local_execution(&id(1), &id(2), &id(700))
                .await
                .unwrap();
            let path = root.0.join(relative(&id(1), &id(1800)));
            let mut record: ProgressRecord =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            record.observed_invocation_ids = invalid;
            let bytes = canonical(&record).unwrap();
            std::fs::write(path, &bytes).unwrap();
            sqlx::query("UPDATE local_blobs SET sha256=?,byte_size=? WHERE id=?")
                .bind(digest(&bytes).as_slice())
                .bind(bytes.len() as i64)
                .bind(id(1800).as_str())
                .execute(&mut s.connection)
                .await
                .unwrap();
            assert!(matches!(
                s.reconcile_local_execution(&id(1), &id(2), &id(700)).await,
                Err(TurnError::Conflict)
            ));
            assert!(s.recover_local_controls(&id(1), &id(2)).await.is_err());
            s.close().await.unwrap();
        }
    });
}

#[test]
fn composed_restore_returns_updated_controls_and_the_same_exact_context() {
    native_run(|| async {
        let root = Root::new();
        let mut s = ready(&root).await;
        command(&mut s, "steer", 1000).await;
        let restored = s
            .restore_local_execution(access(), &id(1300))
            .await
            .unwrap();
        assert_eq!(
            restored.snapshot.progress.state.applied_input_cut.value(),
            1
        );
        assert_eq!(
            restored.context,
            s.local_execution_context(&id(1), &id(2), &id(700))
                .await
                .unwrap()
        );
        let before = counts(&mut s).await;
        let replay = s
            .restore_local_execution(access(), &id(1301))
            .await
            .unwrap();
        assert_eq!(restored.context, replay.context);
        assert_eq!(counts(&mut s).await, before);
        s.close().await.unwrap();
    });
}

#[test]
fn unresolved_projection_uses_the_same_combined_cap_as_the_cancellation_wire_type() {
    native_run(|| async {
        let root = Root::new();
        let mut s = ready(&root).await;
        let mut state = s
            .local_execution_snapshot(&id(1), &id(2), &id(700))
            .await
            .unwrap()
            .progress
            .state;
        state.steps_started = 1;
        s.seal_local_step(access(), &tool_step(), &state, &id(810))
            .await
            .unwrap();
        // Adversarial retained rows exercise the projection boundary directly;
        // they are not represented as verified proposals or physical dispatches.
        for n in 0..100 {
            if n == 99 {
                let snap = s
                    .local_execution_snapshot(&id(1), &id(2), &id(700))
                    .await
                    .unwrap();
                assert_eq!(snap.unresolved_invocations.len(), 99);
                assert_eq!(snap.unresolved_model_calls.len(), 1);
                assert!(
                    matches!(cancellation(&snap, id(1500)), TurnOutcome::Cancelled { unresolved_receipt_refs, .. } if unresolved_receipt_refs.as_slice().len() == 100)
                );
            }
            sqlx::query("INSERT INTO local_invocations (id,workspace_id,created_at_us,run_id,step_id,request_origin,tool_binding_key,idempotency_key,arguments_blob_id,arguments_digest,state) VALUES (?,?,?, ?,?,'model',?,?,?,?,'unknown')")
                .bind(id(3000+n).as_str()).bind(id(1).as_str()).bind(now().unix_micros())
                .bind(id(300).as_str()).bind(id(800).as_str()).bind(id(802).as_str())
                .bind(id(3000+n).as_str()).bind(id(801).as_str()).bind([0u8;32].as_slice())
                .execute(&mut s.connection).await.unwrap();
        }
        assert!(matches!(
            s.local_execution_snapshot(&id(1), &id(2), &id(700)).await,
            Err(TurnError::Conflict)
        ));
        s.close().await.unwrap();
    });
}

#[test]
fn future_multi_turn_histories_are_held_without_replaying_under_the_wrong_attempt() {
    native_run(|| async {
        let root = Root::new();
        let mut s = ready(&root).await;
        // A future-schema-shaped projection is intentionally not admitted by
        // publish_local_turn today. Recovery must refuse its execution profile.
        sqlx::query("INSERT INTO local_turns (id,workspace_id,created_at_us,run_id,attempt_id,ordinal,state,input_command_seq,started_at_us,spec_blob_id) SELECT ?,workspace_id,created_at_us,run_id,attempt_id,2,state,input_command_seq,started_at_us,spec_blob_id FROM local_turns")
            .bind(id(2500).as_str()).execute(&mut s.connection).await.unwrap();
        let before = counts(&mut s).await;
        assert!(matches!(
            s.reconcile_local_execution(&id(1), &id(2), &id(700)).await,
            Err(TurnError::Unsupported)
        ));
        let page = s.recover_local_controls(&id(1), &id(2)).await.unwrap();
        assert_eq!(
            page.runs[0].gate,
            super::super::super::recovery::RecoveryGate::HistoryLimit
        );
        assert!(!page.runs[0].history_complete);
        assert_eq!(counts(&mut s).await, before);
        s.close().await.unwrap();
    });
}
