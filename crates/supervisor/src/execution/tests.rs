use super::*;
use avencrew_store_sqlite::CommandAcceptance;
use std::{fs, os::unix::fs::DirBuilderExt, path::PathBuf};

struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "avencrew-fixture-execution-{}",
            uuid::Uuid::now_v7()
        ));
        fs::DirBuilder::new().mode(0o700).create(&p).unwrap();
        Self(p)
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn run<F, W>(work: F)
where
    F: FnOnce() -> W + Send + 'static,
    W: std::future::Future<Output = ()>,
{
    // The supported supervisor uses a current-thread runtime on the native
    // main-thread stack (8 MiB). Exercise that same profile, including debug
    // Serde validation frames, instead of libtest's smaller worker stack.
    std::thread::Builder::new()
        .name("fixture-supervisor".into())
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

fn launch() -> FixtureLaunch {
    serde_json::from_str(include_str!("../../../../fixtures/models/execution.json")).unwrap()
}
async fn setup(root: &Root) -> (Arc<Mutex<LocalStore>>, PreparedFixtureTurn, FixtureProfile) {
    let mut store = LocalStore::open(&root.0).await.unwrap();
    let input = launch();
    let prepared = prepare_fixture_turn(&mut store, &input).await.unwrap();
    (Arc::new(Mutex::new(store)), prepared, input.profile)
}
async fn close(store: Arc<Mutex<LocalStore>>) {
    Arc::try_unwrap(store)
        .ok()
        .expect("no live execution borrower")
        .into_inner()
        .close()
        .await
        .unwrap();
}
#[test]
fn journal_backed_engine_reads_source_retains_outcome_and_replays_after_reopen() {
    run(|| async {
        let root = Root::new();
        let (store, p, profile) = setup(&root).await;
        let outcome = execute_fixture_turn(
            store.clone(),
            p.workspace.clone(),
            p.actor.clone(),
            p.turn.clone(),
            profile.clone(),
            EngineConfig::default(),
        )
        .await
        .unwrap();
        assert!(
            matches!(outcome, TurnOutcome::CandidateResult { .. }),
            "expected candidate outcome"
        );
        let saved = store
            .lock()
            .await
            .local_execution_snapshot(&p.workspace, &p.actor, &p.turn)
            .await
            .unwrap();
        assert_eq!(saved.progress.state.steps_started, 2);
        let context = store
            .lock()
            .await
            .local_execution_context(&p.workspace, &p.actor, &p.turn)
            .await
            .unwrap();
        assert_eq!(context["history"][0]["invocations"][0]["state"], "result");
        let version = saved.version;
        close(store).await;
        let store = Arc::new(Mutex::new(LocalStore::open(&root.0).await.unwrap()));
        let replay = execute_fixture_turn(
            store.clone(),
            p.workspace.clone(),
            p.actor.clone(),
            p.turn.clone(),
            profile.clone(),
            EngineConfig::default(),
        )
        .await
        .unwrap();
        assert_eq!(replay, outcome);
        assert_eq!(
            store
                .lock()
                .await
                .local_execution_snapshot(&p.workspace, &p.actor, &p.turn)
                .await
                .unwrap()
                .version,
            version
        );
        let TurnOutcome::CandidateResult { artifact_refs, .. } = &outcome else {
            panic!("candidate missing")
        };
        let artifact = &artifact_refs.as_slice()[0];
        let saved = store
            .lock()
            .await
            .read_local_artifact(
                &p.workspace,
                &p.actor,
                &artifact.artifact_id,
                &artifact.version_id,
            )
            .await
            .unwrap();
        fs::write(
            root.0.join(format!(
                "blobs/{}/vault/{}.bin",
                p.workspace.as_str(),
                saved.receipt.content_blob_id.as_str()
            )),
            b"corrupt committed candidate",
        )
        .unwrap();
        // An outcome receipt cannot conceal missing/corrupt checkpoint members.
        assert!(execute_fixture_turn(
            store.clone(),
            p.workspace.clone(),
            p.actor.clone(),
            p.turn.clone(),
            profile,
            EngineConfig::default(),
        )
        .await
        .is_err());
        close(store).await;
    });
}
#[test]
fn accepted_stop_is_available_while_model_waits_and_prevents_tool_dispatch() {
    run(|| async {
        let root = Root::new();
        let (store, p, mut profile) = setup(&root).await;
        profile.episodes[0].delay_ms = 10000;
        let execution = tokio::spawn(execute_fixture_turn(
            store.clone(),
            p.workspace.clone(),
            p.actor.clone(),
            p.turn.clone(),
            profile,
            EngineConfig::default(),
        ));
        let controls = async {
            let started = std::time::Instant::now();
            loop {
                let snap = store
                    .lock()
                    .await
                    .local_execution_snapshot(&p.workspace, &p.actor, &p.turn)
                    .await
                    .unwrap();
                if snap.step.is_some() {
                    break;
                }
                assert!(started.elapsed() < std::time::Duration::from_secs(5));
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            let sample = clock::sample().unwrap();
            let seed = store
                .lock()
                .await
                .read_local_turn(&p.workspace, &p.actor, &p.turn)
                .await
                .unwrap()
                .seed;
            let bytes=serde_json::to_vec(&json!({"kind":"stop","command_id":id(),"run_id":seed.run_spec.run_id,"idempotency_key":"stop-during-model","payload":{"reason":"Fixture stop"}})).unwrap();
            store
                .lock()
                .await
                .accept_command(CommandAcceptance {
                    workspace: &p.workspace,
                    actor: &p.actor,
                    bytes: &bytes,
                    event: &id(),
                    queue: &id(),
                    command_blob: &id(),
                    event_blob: &id(),
                    at: &sample.now,
                })
                .await
                .unwrap();
        };
        controls.await;
        let accepted = std::time::Instant::now();
        let result = execution.await.unwrap();
        assert!(accepted.elapsed() < std::time::Duration::from_secs(5));
        assert!(matches!(result.unwrap(), TurnOutcome::Cancelled { .. }));
        let context = store
            .lock()
            .await
            .local_execution_context(&p.workspace, &p.actor, &p.turn)
            .await
            .unwrap();
        assert_eq!(context["history"][0]["invocations"], json!([]));
        let snap = store
            .lock()
            .await
            .local_execution_snapshot(&p.workspace, &p.actor, &p.turn)
            .await
            .unwrap();
        assert_eq!(snap.progress.state.applied_input_cut.value(), 1);
        close(store).await;
    });
}
#[test]
fn retained_unresolved_intent_never_reissues_inference_and_profile_changes_are_denied() {
    run(|| async {
        use engine::Host;
        let root = Root::new();
        let (store, p, profile) = setup(&root).await;
        let seed = store
            .lock()
            .await
            .read_local_turn(&p.workspace, &p.actor, &p.turn)
            .await
            .unwrap()
            .seed;
        let mut host = FixtureHost {
            store: store.clone(),
            actor: p.actor.clone(),
            seed: seed.clone(),
            compatibility: hash(&record_bytes(&profile).unwrap()),
            profile: profile.clone(),
            active: None,
            active_ordinal: 0,
        };
        let mut state = host.restore(&seed).await.unwrap().state;
        let step = host.seal_step(&seed, &state).await.unwrap();
        state.steps_started = 1;
        assert!(matches!(
            host.model_record(&seed, &step, &state).await.unwrap(),
            ModelRecord::Fresh
        ));
        drop(host);
        let mut changed = profile.clone();
        changed.episodes[0].delay_ms = 1;
        assert!(matches!(
            execute_fixture_turn(
                store.clone(),
                p.workspace.clone(),
                p.actor.clone(),
                p.turn.clone(),
                changed,
                EngineConfig::default()
            )
            .await,
            Err(EngineError::StaleAuthority)
        ));
        let outcome = execute_fixture_turn(
            store.clone(),
            p.workspace.clone(),
            p.actor.clone(),
            p.turn.clone(),
            profile,
            EngineConfig::default(),
        )
        .await
        .unwrap();
        assert!(matches!(
            outcome,
            TurnOutcome::Failed {
                retry_class: avencrew_contracts::wire::TurnOutcomeFailedRetryClass::ReconcileFirst,
                ..
            }
        ));
        let context = store
            .lock()
            .await
            .local_execution_context(&p.workspace, &p.actor, &p.turn)
            .await
            .unwrap();
        assert_eq!(context["history"].as_array().unwrap().len(), 1);
        assert_eq!(context["history"][0]["model_state"], "intent");
        close(store).await;
    });
}
#[test]
fn zero_price_profile_denies_paid_provider_and_cross_tenant_source() {
    run(|| async {
        let root = Root::new();
        let (store, p, mut profile) = setup(&root).await;
        profile.rate_micro = "1".into();
        assert!(execute_fixture_turn(
            store.clone(),
            p.workspace.clone(),
            p.actor.clone(),
            p.turn.clone(),
            profile.clone(),
            EngineConfig::default()
        )
        .await
        .is_err());
        profile.rate_micro = "0".into();
        profile.sources[0].workspace_id = fixed(999);
        let outcome = execute_fixture_turn(
            store.clone(),
            p.workspace.clone(),
            p.actor.clone(),
            p.turn.clone(),
            profile,
            EngineConfig::default(),
        )
        .await
        .unwrap();
        assert!(matches!(outcome, TurnOutcome::Failed { .. }));
        let context = store
            .lock()
            .await
            .local_execution_context(&p.workspace, &p.actor, &p.turn)
            .await
            .unwrap();
        assert_eq!(context["history"][0]["invocations"][0]["state"], "proposed");
        close(store).await;
    });
}

#[test]
fn editing_a_candidate_creates_new_checks_and_rejects_stale_versions() {
    run(|| async {
        use avencrew_store_sqlite::{LocalArtifactPublication, LocalArtifactTarget};
        let root = Root::new();
        let (store, p, profile) = setup(&root).await;
        let outcome = execute_fixture_turn(
            store.clone(),
            p.workspace.clone(),
            p.actor.clone(),
            p.turn.clone(),
            profile,
            EngineConfig::default(),
        )
        .await
        .unwrap();
        let TurnOutcome::CandidateResult { artifact_refs, .. } = outcome else {
            panic!("candidate missing")
        };
        let original = artifact_refs.as_slice()[0].clone();
        let sample = clock::sample().unwrap();
        let body = b"Human correction; review of this exact revision is still required.";
        let validation = avencrew_harness::artifacts::validate(
            avencrew_contracts::runtime::ArtifactFormat::Text,
            body,
            &std::collections::BTreeMap::new(),
        );
        let edited = store
            .lock()
            .await
            .publish_local_artifact(LocalArtifactPublication {
                access: access(&sample, &p.workspace, &p.actor, &p.turn),
                target: LocalArtifactTarget::Edit {
                    artifact: &original.artifact_id,
                    expected_version: &original.version_id,
                },
                version: &id(),
                content_blob: &id(),
                checks_manifest: &id(),
                checks_blob: &id(),
                result: &id(),
                bytes: body,
                validation: &validation,
            })
            .await
            .unwrap();
        assert_ne!(edited.artifact.version_id, original.version_id);
        assert_ne!(edited.artifact.sha256, original.sha256);
        let saved = store
            .lock()
            .await
            .read_local_artifact(
                &p.workspace,
                &p.actor,
                &edited.artifact.artifact_id,
                &edited.artifact.version_id,
            )
            .await
            .unwrap();
        assert_eq!(saved.pending_checks.len(), 1);
        let before = store
            .lock()
            .await
            .diagnostics()
            .await
            .unwrap()
            .counts
            .blobs
            .value();
        assert!(store
            .lock()
            .await
            .publish_local_artifact(LocalArtifactPublication {
                access: access(&sample, &p.workspace, &p.actor, &p.turn),
                target: LocalArtifactTarget::Edit {
                    artifact: &original.artifact_id,
                    expected_version: &original.version_id
                },
                version: &id(),
                content_blob: &id(),
                checks_manifest: &id(),
                checks_blob: &id(),
                result: &id(),
                bytes: body,
                validation: &validation
            })
            .await
            .is_err());
        assert_eq!(
            store
                .lock()
                .await
                .diagnostics()
                .await
                .unwrap()
                .counts
                .blobs
                .value(),
            before
        );
        let original_saved = store
            .lock()
            .await
            .read_local_artifact(
                &p.workspace,
                &p.actor,
                &original.artifact_id,
                &original.version_id,
            )
            .await
            .unwrap();
        assert_ne!(
            original_saved.receipt.check_result_id,
            edited.check_result_id
        );
        // A real accepted stop after the verifying-state human edit must fence
        // further versions, while retaining the already committed revision.
        let seed = store
            .lock()
            .await
            .read_local_turn(&p.workspace, &p.actor, &p.turn)
            .await
            .unwrap()
            .seed;
        let stop=serde_json::to_vec(&json!({"kind":"stop","command_id":id(),"run_id":seed.run_spec.run_id,"idempotency_key":"stop-before-human-edit","payload":{"reason":"Stop editing"}})).unwrap();
        let sample = clock::sample().unwrap();
        store
            .lock()
            .await
            .accept_command(CommandAcceptance {
                workspace: &p.workspace,
                actor: &p.actor,
                bytes: &stop,
                event: &id(),
                queue: &id(),
                command_blob: &id(),
                event_blob: &id(),
                at: &sample.now,
            })
            .await
            .unwrap();
        let before = store
            .lock()
            .await
            .diagnostics()
            .await
            .unwrap()
            .counts
            .blobs
            .value();
        assert!(store
            .lock()
            .await
            .publish_local_artifact(LocalArtifactPublication {
                access: access(&sample, &p.workspace, &p.actor, &p.turn),
                target: LocalArtifactTarget::Edit {
                    artifact: &edited.artifact.artifact_id,
                    expected_version: &edited.artifact.version_id
                },
                version: &id(),
                content_blob: &id(),
                checks_manifest: &id(),
                checks_blob: &id(),
                result: &id(),
                bytes: body,
                validation: &validation,
            })
            .await
            .is_err());
        assert_eq!(
            store
                .lock()
                .await
                .diagnostics()
                .await
                .unwrap()
                .counts
                .blobs
                .value(),
            before
        );
        close(store).await;
        let mut reopened = LocalStore::open(&root.0).await.unwrap();
        let mut read = reopened
            .read_local_artifact(
                &p.workspace,
                &p.actor,
                &edited.artifact.artifact_id,
                &edited.artifact.version_id,
            )
            .await
            .unwrap();
        use std::io::Read;
        let mut bytes = Vec::new();
        read.file.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, body);
        let file = root.0.join(format!(
            "blobs/{}/vault/{}.bin",
            p.workspace.as_str(),
            edited.content_blob_id.as_str()
        ));
        fs::write(file, b"corrupt").unwrap();
        assert!(reopened
            .read_local_artifact(
                &p.workspace,
                &p.actor,
                &edited.artifact.artifact_id,
                &edited.artifact.version_id
            )
            .await
            .is_err());
        reopened.close().await.unwrap();
    });
}

#[test]
fn new_evidence_reaches_a_fourth_model_step_but_repeated_reads_hit_no_progress() {
    run(|| async {
        for distinct in [true, false] {
            let root = Root::new();
            let (store, p, mut profile) = setup(&root).await;
            let read = profile.episodes[0].clone();
            let final_step = profile.episodes[1].clone();
            let source = profile.sources[0].clone();
            profile.sources.clear();
            profile.episodes.clear();
            let reads = if distinct { 3 } else { 4 };
            for n in 0..reads {
                let mut source = source.clone();
                if distinct {
                    source.name = format!("evidence-{n}");
                    source.version_id = id();
                }
                let mut episode = read.clone();
                for event in &mut episode.input.events {
                    if let avencrew_harness::model::fixture::FixtureEventBody::ArgumentsDelta {
                        bytes,
                        ..
                    } = &mut event.body
                    {
                        *bytes = serde_json::to_vec(&json!({"source_name":source.name})).unwrap();
                    }
                }
                if distinct || n == 0 {
                    profile.sources.push(source);
                }
                profile.episodes.push(episode);
            }
            profile.episodes.push(final_step);
            let outcome = execute_fixture_turn(
                store.clone(),
                p.workspace.clone(),
                p.actor.clone(),
                p.turn.clone(),
                profile,
                EngineConfig::default(),
            )
            .await
            .unwrap();
            if distinct {
                assert!(matches!(outcome, TurnOutcome::CandidateResult { .. }));
            } else {
                assert!(
                    matches!(outcome,TurnOutcome::Failed {error,..} if error.code==avencrew_contracts::wire::ErrorCode::ResourceLimit)
                );
            }
            let saved = store
                .lock()
                .await
                .local_execution_snapshot(&p.workspace, &p.actor, &p.turn)
                .await
                .unwrap();
            assert_eq!(saved.progress.state.steps_started, 4);
            assert_eq!(
                saved.progress.state.no_progress_steps,
                if distinct { 0 } else { 3 }
            );
            close(store).await;
        }
    });
}
