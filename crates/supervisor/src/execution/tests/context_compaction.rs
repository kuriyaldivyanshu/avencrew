use super::*;

#[test]
fn context_compaction_consumes_smaller_sealed_request_and_replays_without_dispatch() {
    run(|| async {
        let root = Root::new();
        let (store, p, mut profile) = setup(&root).await;
        profile.sources[0].content = json!({"fraction":1.25,"evidence":"x".repeat(7000)});
        let read = profile.episodes[0].clone();
        let final_episode = profile.episodes[1].clone();
        profile.episodes = vec![read.clone(), read.clone(), read, final_episode];
        let config = EngineConfig {
            context_bytes: 54_000,
            no_progress_steps: 8,
        };
        let outcome = execute_fixture_turn(
            store.clone(),
            p.workspace.clone(),
            p.actor.clone(),
            p.turn.clone(),
            profile.clone(),
            config,
        )
        .await
        .unwrap();
        assert!(matches!(outcome, TurnOutcome::CandidateResult { .. }));
        let context = store
            .lock()
            .await
            .local_execution_context(&p.workspace, &p.actor, &p.turn)
            .await
            .unwrap();
        let last = context["history"].as_array().unwrap().last().unwrap();
        let step_id: DomainId = serde_json::from_value(last["step_id"].clone()).unwrap();
        let saved = store
            .lock()
            .await
            .read_local_step(&p.workspace, &p.actor, &p.turn, &step_id)
            .await
            .unwrap();
        assert!(saved.request_bytes().unwrap().len() + 32768 <= 54_000);
        let reference = saved.source_proof.as_ref().unwrap();
        assert!(context["members"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["id"] == json!(reference.id)));
        let proof_bytes = fs::read(root.0.join(format!(
            "blobs/{}/{}.json",
            p.workspace.as_str(),
            reference.id.as_str()
        )))
        .unwrap();
        let proof: Value = serde_json::from_slice(&proof_bytes).unwrap();
        assert_eq!(
            avencrew_contracts::runtime::context::fixture_context_view(
                &proof,
                reference,
                &saved.compatibility_digest
            )
            .unwrap(),
            saved.request
        );
        let mut unbounded = proof.clone();
        unbounded["context_bytes"] = json!(1_048_576);
        let exact = avencrew_contracts::runtime::context::fixture_context_view(
            &unbounded,
            reference,
            &saved.compatibility_digest,
        )
        .unwrap();
        assert!(record_bytes(&exact).unwrap().len() > saved.request_bytes().unwrap().len());
        assert_eq!(hash(&saved.request_bytes().unwrap()), saved.request_digest);
        println!(
            "compact request={} bytes, exact view={} bytes, output headroom=32768, budget=54000",
            saved.request_bytes().unwrap().len(),
            record_bytes(&exact).unwrap().len()
        );
        assert!(saved.request["source_inventory"].is_object());
        assert_eq!(
            saved.request["anchors"]["coverage"]["effects"],
            "fixture_reads_only"
        );
        assert_eq!(saved.request["anchors"]["seed"]["turn_id"], json!(p.turn));
        assert_eq!(
            saved.request["history"].as_array().unwrap().last().unwrap()["invocations"][0]
                ["result"]["data"]["value"]["content"]["fraction"],
            1.25
        );
        assert!(
            saved.request["history"][0]["invocations"][0]["result"]
                .get("retrieval")
                .is_some()
                || saved.request["summary"].is_object()
        );
        let snapshot = store
            .lock()
            .await
            .local_execution_snapshot(&p.workspace, &p.actor, &p.turn)
            .await
            .unwrap();
        let sample = clock::sample().unwrap();
        let backup_root = Root::new();
        let destination = fs::canonicalize(&backup_root.0)
            .unwrap()
            .join("proof-backup");
        store
            .lock()
            .await
            .create_backup(&destination, &id(), &sample.now)
            .await
            .unwrap();
        LocalStore::inspect_backup(&destination).await.unwrap();
        close(store).await;
        let store = Arc::new(Mutex::new(LocalStore::open(&root.0).await.unwrap()));
        let replay = execute_fixture_turn(
            store.clone(),
            p.workspace.clone(),
            p.actor.clone(),
            p.turn.clone(),
            profile,
            config,
        )
        .await
        .unwrap();
        assert_eq!(replay, outcome);
        let restored = store
            .lock()
            .await
            .local_execution_snapshot(&p.workspace, &p.actor, &p.turn)
            .await
            .unwrap();
        assert_eq!(restored.version, snapshot.version);
        assert_eq!(restored.progress.state.steps_started, 4);
        assert_eq!(
            store
                .lock()
                .await
                .read_local_step(&p.workspace, &p.actor, &p.turn, &step_id)
                .await
                .unwrap()
                .request,
            saved.request
        );
        close(store).await;
    });
}

#[test]
fn context_compaction_insufficient_output_headroom_yields_before_intent() {
    run(|| async {
        let root = Root::new();
        let (store, p, profile) = setup(&root).await;
        let outcome = execute_fixture_turn(
            store.clone(),
            p.workspace.clone(),
            p.actor.clone(),
            p.turn.clone(),
            profile,
            EngineConfig {
                context_bytes: 32768,
                ..EngineConfig::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(
            serde_json::to_value(&outcome).unwrap()["reason"],
            "context_limit"
        );
        let snap = store
            .lock()
            .await
            .local_execution_snapshot(&p.workspace, &p.actor, &p.turn)
            .await
            .unwrap();
        assert_eq!(snap.progress.state.steps_started, 0);
        assert!(snap.unresolved_model_calls.is_empty());
        assert!(snap.unresolved_invocations.is_empty());
        close(store).await;
    });
}
