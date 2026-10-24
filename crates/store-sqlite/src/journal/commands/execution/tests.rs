use super::super::tests::{register, setup};
use super::super::turns::tests::{publish, seed as prepared_seed};
use super::*;
use crate::journal::tests::{run, Root};
use avencrew_contracts::runtime::FixtureResult;
use serde_json::json;

fn id(n: u32) -> DomainId {
    DomainId::new(format!("01900000-0000-7000-8000-{n:012x}")).unwrap()
}
fn now() -> Instant {
    Instant::new("2026-10-11T00:00:00.000000Z").unwrap()
}
fn seed() -> avencrew_contracts::wire::RpcRequestHarnessStartTurnBody {
    let mut seed = prepared_seed();
    seed.lease.monotonic_deadline_ns = Counter::new("1000000000000").unwrap();
    seed
}
fn access() -> LocalExecutionAccess<'static> {
    static VALUES: std::sync::OnceLock<(DomainId, DomainId, DomainId, Instant, DomainId, Counter)> =
        std::sync::OnceLock::new();
    let v = VALUES.get_or_init(|| {
        (
            id(1),
            id(2),
            id(700),
            now(),
            id(704),
            Counter::new("10").unwrap(),
        )
    });
    LocalExecutionAccess::new(&v.0, &v.1, &v.2, &v.3, &v.4, &v.5)
}

fn step() -> LocalStep {
    let request = json!({"objective_version_id":seed().run_spec.objective_version_id,"accepted_inputs":[],"instructions":[],"profile":"fixture","monetary_cap_micro":"0"});
    let mut catalog = b"avencrew.sealed-catalog/1\0".to_vec();
    catalog.extend(b"[]");
    let catalog_digest = hex(&digest(&catalog));
    LocalStep {
        accepted_input_cut: Counter::new("0").unwrap(),
        candidate:None,
        binding: serde_json::from_value(json!({"step_id":id(800),"model_profile_id":id(750),"model_adapter_digest":"0".repeat(64),"prompt_schema_version":"1.0","catalog_digest":catalog_digest,"tool_binding_ids":[],"instruction_digests":[],"context_manifest_id":id(801),"environment_capability_digest":"0".repeat(64),"permission_generation":"1","deletion_generation":"0","sealed_at":now()})).unwrap(),
        context: serde_json::from_value(json!({"workspace_id":id(1),"project_id":null,"run_id":id(300),"attempt_id":id(703),"step_id":id(800),"principal_id":id(3),"epoch":"1","environment_id":null,"generation":null,"binding_id":id(802),"catalog_digest":catalog_digest,"budget_hold_refs":[],"approval_ref":null})).unwrap(),
        lease: seed().lease, bindings: vec![], model_call_id:id(803), request_digest: avencrew_contracts::scalars::Digest::new(hex(&digest(&canonical(&request).unwrap()))).unwrap(),
        provider_profile_id:id(750), compatibility_digest:avencrew_contracts::scalars::Digest::new("0".repeat(64)).unwrap(), request,
    }
}
fn result() -> ModelCompletion {
    Ok(FixtureResult { response: serde_json::from_value(json!({"model_call_id":id(803),"step_id":id(800),"items":[{"kind":"text","text":"Candidate; no completion accepted"}],"finish":"final","usage_refs":[],"diagnostic_error":null})).unwrap(), usage:FixtureUsage::Unknown, proposal_identities:vec![] })
}

fn native_run<F, W>(work: F)
where
    F: FnOnce() -> W + Send + 'static,
    W: std::future::Future<Output = ()>,
{
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(work())
        })
        .unwrap()
        .join()
        .unwrap();
}
fn tool_step() -> LocalStep {
    use avencrew_contracts::runtime::{Binding, Effect};
    let mut s = step();
    s.bindings = vec![Binding {
        binding_id: id(802),
        name: "fixture.read".into(),
        capability_version_id: id(830),
        adapter_digest: avencrew_contracts::scalars::Digest::new("0".repeat(64)).unwrap(),
        effect: Effect::Read,
        input_schema: json!({"type":"object","additionalProperties":false,"properties":{"source_name":{"type":"string"},"ratio":{"type":"number"}},"required":["source_name","ratio"]}),
        output_schema: json!({"type":"object","additionalProperties":false,"properties":{"ratio":{"type":"number"}},"required":["ratio"]}),
    }];
    let mut bytes = b"avencrew.sealed-catalog/1\0".to_vec();
    bytes.extend(canonical(&s.bindings).unwrap());
    let hash = avencrew_contracts::scalars::Digest::new(hex(&digest(&bytes))).unwrap();
    s.binding.catalog_digest = hash.clone();
    s.context.catalog_digest = hash;
    s.binding.tool_binding_ids = avencrew_contracts::scalars::List::new(vec![id(802)]).unwrap();
    s
}
fn tool_completion() -> ModelCompletion {
    let mut r = result().unwrap();
    r.response.finish = NormalizedModelResponseFinish::Tools;
    r.response.items=serde_json::from_value(json!([{"kind":"tool_proposal","provider_call_id":"read-source","binding_id":id(802),"arguments":{"source_name":"synthetic","ratio":1.25}}])).unwrap();
    r.proposal_identities = vec![avencrew_contracts::runtime::ProposalIdentity {
        item: 0,
        provider_call_id: "read-source".into(),
        invocation_id: id(820),
    }];
    Ok(r)
}
fn proposal() -> avencrew_contracts::runtime::LocalProposal {
    use avencrew_contracts::runtime::{Effect, LocalProposal, Origin};
    let s = tool_step();
    let arguments =
        serde_json::from_value(json!({"source_name":"synthetic","ratio":1.25})).unwrap();
    let hash = avencrew_contracts::runtime::content_digest(
        &avencrew_contracts::runtime::record_bytes(&arguments).unwrap(),
    );
    let mut p = LocalProposal {
        invocation_id: id(820),
        context: s.context,
        origin: Origin::Model {
            model_call_id: s.model_call_id,
            provider_call_id: "read-source".into(),
        },
        arguments,
        arguments_digest: hash,
        adapter_digest: s.bindings[0].adapter_digest.clone(),
        effect: Effect::Read,
        digest: avencrew_contracts::scalars::Digest::new("0".repeat(64)).unwrap(),
    };
    p.digest = p.calculated_digest().unwrap();
    p
}
fn tool_result() -> avencrew_contracts::wire::ToolResult {
    serde_json::from_value(json!({"status":"succeeded","invocation_id":id(820),"schema_version":"1.0","artifact_refs":[],"receipt_ref":null,"truncated":false,"next_cursor":null,"usage_refs":[],"data":{"binding_id":id(802),"value":{"ratio":1.25}}})).unwrap()
}
async fn stage(root: &Root, name: &str) {
    let mut s = ready(root).await;
    if !name.starts_with("intent_") {
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
        if !name.starts_with("response_") {
            s.record_local_model(
                access(),
                &tool_step(),
                &tool_completion(),
                &state,
                &id(812),
                &id(813),
            )
            .await
            .unwrap();
            s.commit_local_proposals(access(), &[proposal()], &id(814))
                .await
                .unwrap();
            if name.starts_with("result_") {
                assert!(matches!(
                    s.claim_local_dispatch(
                        access(),
                        &proposal(),
                        &avencrew_contracts::scalars::Digest::new("0".repeat(64)).unwrap(),
                        &id(815)
                    )
                    .await
                    .unwrap(),
                    LocalDispatch::Fresh
                ));
            }
        }
    }
    s.close().await.unwrap();
}
#[test]
fn execution_crash_child() {
    let Ok(root) = std::env::var("AVENCREW_EXECUTION_CRASH_ROOT") else {
        return;
    };
    let stage = std::env::var("AVENCREW_EXECUTION_CRASH_STAGE").unwrap();
    native_run(move || async move {
        let mut s = LocalStore::open(std::path::Path::new(&root)).await.unwrap();
        let mut state = s
            .local_execution_snapshot(&id(1), &id(2), &id(700))
            .await
            .unwrap()
            .progress
            .state;
        if stage.starts_with("intent_") {
            state.steps_started = 1;
            s.seal_local_step(access(), &tool_step(), &state, &id(810))
                .await
                .unwrap();
        } else if stage.starts_with("response_") {
            s.record_local_model(
                access(),
                &tool_step(),
                &tool_completion(),
                &state,
                &id(812),
                &id(813),
            )
            .await
            .unwrap();
        } else if stage.starts_with("dispatch_") {
            s.claim_local_dispatch(
                access(),
                &proposal(),
                &avencrew_contracts::scalars::Digest::new("0".repeat(64)).unwrap(),
                &id(815),
            )
            .await
            .unwrap();
        } else {
            s.record_local_tool_result(access(), &proposal(), Some(&tool_result()), &id(816))
                .await
                .unwrap();
        }
        panic!("crash barrier did not hold the child");
    });
}
#[test]
fn actual_process_kills_at_intent_response_dispatch_and_result_commits_preserve_only_committed_truth(
) {
    native_run(|| async {
        for name in [
            "intent_before_commit",
            "intent_after_commit",
            "response_before_commit",
            "response_after_commit",
            "dispatch_before_commit",
            "dispatch_after_commit",
            "result_before_commit",
            "result_after_commit",
        ] {
            let root = Root::new();
            stage(&root, name).await;
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
                    panic!("child did not reach {name}");
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            child.kill().unwrap();
            child.wait().unwrap();
            let mut s = LocalStore::open(&root.0).await.unwrap();
            let after = name.contains("after_commit");
            if name.starts_with("intent_") {
                let snap = s
                    .local_execution_snapshot(&id(1), &id(2), &id(700))
                    .await
                    .unwrap();
                assert_eq!(snap.step.is_some(), after);
                assert!(snap.model.is_none());
                if after {
                    assert!(!s
                        .seal_local_step(access(), &tool_step(), &snap.progress.state, &id(811))
                        .await
                        .unwrap());
                }
            } else if name.starts_with("response_") {
                let snap = s
                    .local_execution_snapshot(&id(1), &id(2), &id(700))
                    .await
                    .unwrap();
                assert_eq!(snap.model.is_some(), after);
                if after {
                    assert!(snap.model == Some(tool_completion()));
                }
            } else if name.starts_with("dispatch_") {
                let dispatch = s
                    .claim_local_dispatch(
                        access(),
                        &proposal(),
                        &avencrew_contracts::scalars::Digest::new("0".repeat(64)).unwrap(),
                        &id(815),
                    )
                    .await
                    .unwrap();
                assert!(if after {
                    matches!(dispatch, LocalDispatch::Unknown)
                } else {
                    matches!(dispatch, LocalDispatch::Fresh)
                });
            } else {
                let dispatch = s
                    .claim_local_dispatch(
                        access(),
                        &proposal(),
                        &avencrew_contracts::scalars::Digest::new("0".repeat(64)).unwrap(),
                        &id(815),
                    )
                    .await
                    .unwrap();
                if after {
                    match dispatch {
                        LocalDispatch::Result(result) => assert_eq!(*result, tool_result()),
                        _ => panic!("lost committed result"),
                    }
                } else {
                    assert!(matches!(dispatch, LocalDispatch::Unknown));
                }
            }
            let budget = s
                .local_execution_budget(&id(1), &id(2), &id(700))
                .await
                .unwrap();
            assert!(!budget.overdrawn);
            s.close().await.unwrap();
        }
    });
}
#[test]
fn orphan_blob_bytes_and_expiry_during_publication_cannot_escape_admission() {
    native_run(|| async {
        use std::os::unix::fs::OpenOptionsExt;
        let root = Root::new();
        let mut s = ready(&root).await;
        let path = root.0.join(format!(
            "blobs/{}/{}.json",
            id(1).as_str(),
            id(899).as_str()
        ));
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .unwrap();
        file.set_len(64 * 1024 * 1024 + 1).unwrap();
        drop(file);
        let budget = s
            .local_execution_budget(&id(1), &id(2), &id(700))
            .await
            .unwrap();
        assert!(budget.overdrawn);
        assert!(!budget.can_start_model);
        s.close().await.unwrap();
        let root = Root::new();
        let mut s = ready(&root).await;
        let mut state = s
            .local_execution_snapshot(&id(1), &id(2), &id(700))
            .await
            .unwrap()
            .progress
            .state;
        state.steps_started = 1;
        let deadline = Counter::new("999999999999").unwrap();
        let mut a = access();
        a.monotonic_now = &deadline;
        assert!(matches!(
            s.seal_local_step(a, &step(), &state, &id(810)).await,
            Err(TurnError::StaleAuthority)
        ));
        assert_eq!(
            s.local_execution_snapshot(&id(1), &id(2), &id(700))
                .await
                .unwrap()
                .progress
                .state
                .steps_started,
            0
        );
        s.close().await.unwrap();
    });
}
pub(super) fn barrier(database: &std::path::Path, stage: &str) -> Result<(), TurnError> {
    if std::env::var("AVENCREW_EXECUTION_CRASH_STAGE").as_deref() != Ok(stage) {
        return Ok(());
    }
    std::fs::write(
        database.parent().unwrap().join("execution-crash-ready"),
        stage,
    )
    .map_err(StoreError::from)?;
    let start = std::time::Instant::now();
    while start.elapsed() < std::time::Duration::from_secs(30) {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    Err(TurnError::Conflict)
}
async fn ready(root: &Root) -> LocalStore {
    let mut store = setup(root).await;
    register(&mut store).await.unwrap();
    publish(&mut store, &seed()).await.unwrap();
    store
}
#[test]
fn exact_context_rejects_omitted_members_and_retained_invocation_projection_drift() {
    native_run(|| async {
        let root = Root::new();
        stage(&root, "result_before_commit").await;
        let mut s = LocalStore::open(&root.0).await.unwrap();
        s.record_local_tool_result(access(), &proposal(), Some(&tool_result()), &id(816))
            .await
            .unwrap();
        let context = s
            .local_execution_context(&id(1), &id(2), &id(700))
            .await
            .unwrap();
        let snap = s
            .local_execution_snapshot(&id(1), &id(2), &id(700))
            .await
            .unwrap();
        let manifest = json!({
            "schema_version":"avencrew.execution-manifest/1", "workspace_id":id(1),
            "turn_id":id(700), "run_id":seed().run_spec.run_id,
            "attempt_id":seed().lease.attempt_id, "epoch":seed().lease.epoch,
            "seed_sha256":hex(&digest(&canonical(&seed()).unwrap())),
            "state":snap.progress.state, "pending_step_id":snap.progress.pending_step_id,
            "members":context["members"]
        });
        s.verify_execution_manifest(&id(1), &id(2), &manifest, true)
            .await
            .unwrap();
        let mut missing = manifest.clone();
        missing["members"]
            .as_array_mut()
            .unwrap()
            .retain(|m| m["id"] != json!(id(812)));
        assert!(s
            .verify_execution_manifest(&id(1), &id(2), &missing, true)
            .await
            .is_err());
        // The new normalized records contain fractional tool arguments/results;
        // backup inspection must preserve them without integral canonicalization.
        let packages = Root::new();
        let package = packages.0.join("execution");
        s.create_backup(&package, &id(850), &now()).await.unwrap();
        assert_eq!(
            LocalStore::inspect_backup(&package)
                .await
                .unwrap()
                .journal_schema_version,
            4
        );
        sqlx::query("UPDATE local_invocations SET provider_call_id='changed-provider-identity'")
            .execute(&mut s.connection)
            .await
            .unwrap();
        s.close().await.unwrap();
        let mut s = LocalStore::open(&root.0).await.unwrap();
        assert!(s
            .local_execution_context(&id(1), &id(2), &id(700))
            .await
            .is_err());
        s.close().await.unwrap();
    });
}
#[test]
fn sealed_intent_response_and_unknown_usage_survive_reopen_without_readmission() {
    run(async {
        let root = Root::new();
        let mut store = ready(&root).await;
        let mut state = store
            .local_execution_snapshot(&id(1), &id(2), &id(700))
            .await
            .unwrap()
            .progress
            .state;
        state.steps_started = 1;
        assert!(store
            .seal_local_step(access(), &step(), &state, &id(810))
            .await
            .unwrap());
        assert!(!store
            .seal_local_step(access(), &step(), &state, &id(811))
            .await
            .unwrap());
        store.close().await.unwrap();
        let mut store = LocalStore::open(&root.0).await.unwrap();
        let snapshot = store
            .local_execution_snapshot(&id(1), &id(2), &id(700))
            .await
            .unwrap();
        assert!(snapshot.step.is_some());
        assert!(snapshot.model.is_none());
        assert_eq!(snapshot.version, 1);
        assert!(!store
            .seal_local_step(access(), &step(), &state, &id(811))
            .await
            .unwrap());
        store
            .record_local_model(access(), &step(), &result(), &state, &id(812), &id(813))
            .await
            .unwrap();
        store
            .record_local_model(access(), &step(), &result(), &state, &id(812), &id(813))
            .await
            .unwrap();
        store.close().await.unwrap();
        let mut store = LocalStore::open(&root.0).await.unwrap();
        let snapshot = store
            .local_execution_snapshot(&id(1), &id(2), &id(700))
            .await
            .unwrap();
        assert!(snapshot.model == Some(result()));
        assert_eq!(snapshot.version, 2);
        let tokens: Option<i64> = sqlx::query_scalar("SELECT input_tokens FROM local_model_calls")
            .fetch_one(&mut store.connection)
            .await
            .unwrap();
        assert!(tokens.is_none());
        store.close().await.unwrap();
    });
}
#[test]
fn changed_scope_request_clock_or_stop_cannot_admit_a_model_intent() {
    run(async {
        for case in 0..5 {
            let root = Root::new();
            let mut store = ready(&root).await;
            let mut state = store
                .local_execution_snapshot(&id(1), &id(2), &id(700))
                .await
                .unwrap()
                .progress
                .state;
            state.steps_started = 1;
            let mut step = step();
            let wrong_boot = id(999);
            let expired = Counter::new("1000000000000").unwrap();
            let mut a = access();
            match case {
                0 => step.context.run_id = id(999),
                1 => step.request["monetary_cap_micro"] = json!("1"),
                2 => a.boot = &wrong_boot,
                3 => a.monotonic_now = &expired,
                _ => {
                    sqlx::query(
                        "UPDATE local_runs SET cancellation_requested=1,status='cancelling'",
                    )
                    .execute(&mut store.connection)
                    .await
                    .unwrap();
                }
            }
            assert!(store
                .seal_local_step(a, &step, &state, &id(810))
                .await
                .is_err());
            let count: i64 = sqlx::query_scalar("SELECT count(*) FROM local_model_calls")
                .fetch_one(&mut store.connection)
                .await
                .unwrap();
            assert_eq!(count, 0);
            store.close().await.unwrap();
        }
    });
}
#[test]
fn transaction_abort_never_acknowledges_inference_and_corrupt_seals_block_replay() {
    run(async {
        let root = Root::new();
        let mut store = ready(&root).await;
        let mut state = store
            .local_execution_snapshot(&id(1), &id(2), &id(700))
            .await
            .unwrap()
            .progress
            .state;
        state.steps_started = 1;
        sqlx::query("CREATE TEMP TRIGGER reject_model BEFORE INSERT ON local_model_calls BEGIN SELECT RAISE(ABORT,'cut'); END").execute(&mut store.connection).await.unwrap();
        assert!(store
            .seal_local_step(access(), &step(), &state, &id(810))
            .await
            .is_err());
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM local_steps")
            .fetch_one(&mut store.connection)
            .await
            .unwrap();
        assert_eq!(count, 0);
        sqlx::query("DROP TRIGGER reject_model")
            .execute(&mut store.connection)
            .await
            .unwrap();
        assert!(store
            .seal_local_step(access(), &step(), &state, &id(810))
            .await
            .unwrap());
        std::fs::write(
            root.0.join(format!(
                "blobs/{}/{}.json",
                id(1).as_str(),
                id(801).as_str()
            )),
            b"{}",
        )
        .unwrap();
        assert!(store
            .local_execution_snapshot(&id(1), &id(2), &id(700))
            .await
            .is_err());
        store.close().await.unwrap();
    });
}

async fn candidate(root: &Root) -> (LocalStore, LocalArtifactReceipt) {
    use avencrew_contracts::runtime::{
        content_digest, ArtifactFormat, ArtifactValidation, CandidateAllocation, CheckStatus,
    };
    let mut store = ready(root).await;
    let mut sealed = step();
    sealed.candidate = Some(CandidateAllocation {
        artifact_id: id(900),
        version_id: id(901),
        content_blob_id: id(902),
        checks_manifest_id: id(903),
        checks_blob_id: id(904),
        check_result_id: id(905),
    });
    let mut state = store
        .local_execution_snapshot(&id(1), &id(2), &id(700))
        .await
        .unwrap()
        .progress
        .state;
    state.steps_started = 1;
    store
        .seal_local_step(access(), &sealed, &state, &id(810))
        .await
        .unwrap();
    store
        .record_local_model(access(), &sealed, &result(), &state, &id(812), &id(813))
        .await
        .unwrap();
    let body = b"Candidate; no completion accepted";
    let validation = ArtifactValidation {
        schema_version: "avencrew.artifact-validation/1".into(),
        format: ArtifactFormat::Text,
        sha256: content_digest(body),
        byte_size: Counter::new(body.len().to_string()).unwrap(),
        status: CheckStatus::Passed,
        codes: vec!["nonempty_utf8".into()],
        base_sha256: None,
        result_sha256: None,
    };
    let receipt = store
        .publish_local_artifact(LocalArtifactPublication {
            access: access(),
            target: LocalArtifactTarget::Candidate {
                step: &id(800),
                artifact: &id(900),
            },
            version: &id(901),
            content_blob: &id(902),
            checks_manifest: &id(903),
            checks_blob: &id(904),
            result: &id(905),
            bytes: body,
            validation: &validation,
        })
        .await
        .unwrap();
    (store, receipt)
}
#[test]
fn human_edits_reject_stop_epoch_and_expired_or_wrong_boot_authority() {
    native_run(|| async {
        for case in 0..5 {
            let root = Root::new();
            let (mut s, receipt) = candidate(&root).await;
            let old = s
                .read_local_artifact(&id(1), &id(2), &id(900), &id(901))
                .await
                .unwrap();
            let before: i64 = sqlx::query_scalar("SELECT count(*) FROM local_blobs")
                .fetch_one(&mut s.connection)
                .await
                .unwrap();
            let mut a = access();
            let expired = seed().lease.expires_at;
            let boot = id(999);
            let deadline = seed().lease.monotonic_deadline_ns;
            match case {
                0 => {
                    sqlx::query(
                        "UPDATE local_runs SET cancellation_requested=1,status='cancelling'",
                    )
                    .execute(&mut s.connection)
                    .await
                    .unwrap();
                }
                1 => {
                    sqlx::query("UPDATE local_runs SET fencing_epoch=fencing_epoch+1")
                        .execute(&mut s.connection)
                        .await
                        .unwrap();
                }
                2 => a.now = &expired,
                3 => a.boot = &boot,
                _ => a.monotonic_now = &deadline,
            }
            let result = s
                .publish_local_artifact(LocalArtifactPublication {
                    access: a,
                    target: LocalArtifactTarget::Edit {
                        artifact: &receipt.artifact.artifact_id,
                        expected_version: &receipt.artifact.version_id,
                    },
                    version: &id(910),
                    content_blob: &id(911),
                    checks_manifest: &id(912),
                    checks_blob: &id(913),
                    result: &id(914),
                    bytes: b"Candidate; no completion accepted",
                    validation: &old.validation,
                })
                .await;
            assert!(
                result.is_err(),
                "edit accepted after authority change {case}"
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM local_blobs")
                    .fetch_one(&mut s.connection)
                    .await
                    .unwrap(),
                before
            );
            assert_eq!(
                sqlx::query_scalar::<_, String>("SELECT current_version_id FROM local_artifacts")
                    .fetch_one(&mut s.connection)
                    .await
                    .unwrap(),
                id(901).as_str()
            );
            s.close().await.unwrap();
        }
    });
}

#[test]
fn committed_evidence_updates_progress_and_reopen_keeps_the_reset_counter() {
    native_run(|| async {
        let root = Root::new();
        stage(&root, "result_before_commit").await;
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
        let changed = s
            .observe_local_step(access(), &id(800), &[id(820)], &state, &id(817))
            .await
            .unwrap();
        assert_ne!(
            changed, state.progress_digest,
            "new verified evidence never updates progress"
        );
        s.close().await.unwrap();
        let mut s = LocalStore::open(&root.0).await.unwrap();
        let saved = s
            .local_execution_snapshot(&id(1), &id(2), &id(700))
            .await
            .unwrap();
        assert_eq!(saved.progress.state.progress_digest, changed);
        assert_eq!(saved.progress.state.no_progress_steps, 0);
        s.close().await.unwrap();
    });
}

#[test]
fn repeated_evidence_increments_no_progress_and_new_evidence_resets_it() {
    native_run(|| async {
        let root = Root::new();
        let mut s = ready(&root).await;
        let mut previous = None;
        for (n, ratio) in [1.25, 1.25, 2.25, 2.25].into_iter().enumerate() {
            let offset = n as u32 * 1000;
            let i = |x| id(x + offset);
            let mut step = tool_step();
            step.binding.step_id = i(800);
            step.context.step_id = i(800);
            step.binding.context_manifest_id = i(801);
            step.model_call_id = i(803);
            let mut completion = tool_completion().unwrap();
            completion.response.step_id = i(800);
            completion.response.model_call_id = i(803);
            completion.proposal_identities[0].invocation_id = i(820);
            let mut p = proposal();
            p.invocation_id = i(820);
            p.context = step.context.clone();
            p.origin = avencrew_contracts::runtime::Origin::Model {
                model_call_id: i(803),
                provider_call_id: "read-source".into(),
            };
            p.digest = p.calculated_digest().unwrap();
            let mut state = s
                .local_execution_snapshot(&id(1), &id(2), &id(700))
                .await
                .unwrap()
                .progress
                .state;
            state.steps_started += 1;
            s.seal_local_step(access(), &step, &state, &i(810))
                .await
                .unwrap();
            s.record_local_model(access(), &step, &Ok(completion), &state, &i(812), &i(813))
                .await
                .unwrap();
            s.commit_local_proposals(access(), &[p.clone()], &i(814))
                .await
                .unwrap();
            assert!(matches!(
                s.claim_local_dispatch(
                    access(),
                    &p,
                    &avencrew_contracts::scalars::Digest::new("0".repeat(64)).unwrap(),
                    &i(815)
                )
                .await
                .unwrap(),
                LocalDispatch::Fresh
            ));
            let result=serde_json::from_value(json!({"status":"succeeded","invocation_id":i(820),"schema_version":"1.0","artifact_refs":[],"receipt_ref":null,"truncated":false,"next_cursor":null,"usage_refs":[],"data":{"binding_id":id(802),"value":{"ratio":ratio}}})).unwrap();
            s.record_local_tool_result(access(), &p, Some(&result), &i(816))
                .await
                .unwrap();
            let digest = s
                .observe_local_step(access(), &i(800), &[i(820)], &state, &i(817))
                .await
                .unwrap();
            let state = s
                .local_execution_snapshot(&id(1), &id(2), &id(700))
                .await
                .unwrap()
                .progress
                .state;
            assert_eq!(state.no_progress_steps, (n % 2) as u32);
            if n % 2 == 1 {
                assert_eq!(previous.as_ref(), Some(&digest));
            } else {
                assert_ne!(previous.as_ref(), Some(&digest));
            }
            previous = Some(digest);
        }
        s.close().await.unwrap();
    });
}
#[test]
fn dropped_tool_future_retains_typed_unknown_and_never_overwrites_known_result() {
    native_run(|| async {
        use avencrew_contracts::wire::{ErrorCode, ToolResult};
        for known in [false, true] {
            let root = Root::new();
            stage(&root, "result_before_commit").await;
            let mut s = LocalStore::open(&root.0).await.unwrap();
            if known {
                s.record_local_tool_result(access(), &proposal(), Some(&tool_result()), &id(816))
                    .await
                    .unwrap();
            }
            s.record_local_unknown_invocation(
                access(),
                &id(800),
                &id(820),
                ErrorCode::Timeout,
                &id(818),
            )
            .await
            .unwrap();
            let saved = s
                .read_local_tool_result(&id(1), &id(2), &id(820))
                .await
                .unwrap();
            if known {
                assert_eq!(saved, tool_result());
            } else {
                assert!(
                    matches!(saved,ToolResult::Unknown {error,..} if error.code==ErrorCode::Timeout)
                );
            }
            let state = s
                .local_execution_snapshot(&id(1), &id(2), &id(700))
                .await
                .unwrap()
                .progress
                .state;
            s.observe_local_step(access(), &id(800), &[id(820)], &state, &id(817))
                .await
                .unwrap();
            s.close().await.unwrap();
            let mut s = LocalStore::open(&root.0).await.unwrap();
            let snap = s
                .local_execution_snapshot(&id(1), &id(2), &id(700))
                .await
                .unwrap();
            assert_eq!(
                snap.unresolved_invocations,
                if known { vec![] } else { vec![id(820)] }
            );
            let saved = s
                .read_local_tool_result(&id(1), &id(2), &id(820))
                .await
                .unwrap();
            assert_eq!(matches!(saved, ToolResult::Unknown { .. }), !known);
            s.close().await.unwrap();
        }
    });
}
#[test]
fn ignored_run_cursor_updates_roll_back_progress_and_dispatch_events() {
    native_run(|| async {
        for dispatch in [false, true] {
            let root = Root::new();
            if dispatch {
                stage(&root, "dispatch_before_commit").await;
            } else {
                ready(&root).await.close().await.unwrap();
            }
            let mut s = LocalStore::open(&root.0).await.unwrap();
            let before: i64 = sqlx::query_scalar("SELECT count(*) FROM local_events")
                .fetch_one(&mut s.connection)
                .await
                .unwrap();
            sqlx::query("CREATE TEMP TRIGGER ignore_run_cursor BEFORE UPDATE OF last_event_seq ON local_runs BEGIN SELECT RAISE(IGNORE); END").execute(&mut s.connection).await.unwrap();
            if dispatch {
                assert!(matches!(
                    s.claim_local_dispatch(
                        access(),
                        &proposal(),
                        &avencrew_contracts::scalars::Digest::new("0".repeat(64)).unwrap(),
                        &id(815)
                    )
                    .await,
                    Err(TurnError::StaleAuthority)
                ));
            } else {
                let mut state = s
                    .local_execution_snapshot(&id(1), &id(2), &id(700))
                    .await
                    .unwrap()
                    .progress
                    .state;
                state.steps_started = 1;
                assert!(matches!(
                    s.seal_local_step(access(), &tool_step(), &state, &id(810))
                        .await,
                    Err(TurnError::StaleAuthority)
                ));
            }
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM local_events")
                    .fetch_one(&mut s.connection)
                    .await
                    .unwrap(),
                before
            );
            s.close().await.unwrap();
        }
    });
}

#[test]
fn artifact_checks_pin_audience_and_reject_digest_valid_cross_audience_records() {
    native_run(|| async {
        let root = Root::new();
        let (mut s, receipt) = candidate(&root).await;
        let saved = s
            .read_local_artifact(&id(1), &id(2), &id(900), &id(901))
            .await
            .unwrap();
        assert_eq!(saved.audience_policy_id, seed().run_spec.audience_policy_id);
        let bytes = s
            .read_registered_blob(
                &id(1),
                &id(2),
                &id(904),
                "application/vnd.avencrew.artifact-checks+json",
            )
            .await
            .unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            value["audience_policy_id"],
            json!(seed().run_spec.audience_policy_id)
        );
        let mut forged = receipt;
        forged.checks_manifest_id = id(950);
        value["receipt"] = serde_json::to_value(&forged).unwrap();
        value["audience_policy_id"] = json!(id(999));
        let bytes = canonical(&value).unwrap();
        s.put_canonical(&id(1), &id(951), &bytes).unwrap();
        // Construct a separate, digest-consistent hostile manifest without
        // weakening any immutable trigger or changing the real version pointer.
        let mut tx = s.connection.begin().await.unwrap();
        blob(
            &mut tx,
            &id(1),
            &id(2),
            &id(951),
            &bytes,
            "application/vnd.avencrew.artifact-checks+json",
            now().unix_micros(),
        )
        .await
        .unwrap();
        sqlx::query("INSERT INTO local_manifests (id,workspace_id,created_at_us,owner_actor_id,kind,schema_version,canonical_blob_id,digest,sealed_at_us,deletion_generation) VALUES (?,?,?,?,'snapshot',1,?,?,?,0)")
            .bind(id(950).as_str()).bind(id(1).as_str()).bind(now().unix_micros()).bind(id(2).as_str()).bind(id(951).as_str()).bind(digest(&bytes).as_slice()).bind(now().unix_micros()).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        assert!(s
            .read_artifact_checks(&id(1), &id(2), &forged)
            .await
            .is_err());
        assert!(s
            .read_local_artifact(&id(1), &id(2), &id(900), &id(901))
            .await
            .is_ok());
        s.close().await.unwrap();
    });
}
