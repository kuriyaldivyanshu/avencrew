use super::*;
use avencrew_broker::Invocation;
use avencrew_harness::{engine::Host as _, model::fixture::commit_fixture_proposals};

async fn stop(
    store: &Arc<Mutex<LocalStore>>,
    p: &PreparedFixtureTurn,
    key: &str,
) -> (Vec<u8>, avencrew_contracts::wire::CommandReceipt) {
    let sample = clock::sample().unwrap();
    let seed = store
        .lock()
        .await
        .read_local_turn(&p.workspace, &p.actor, &p.turn)
        .await
        .unwrap()
        .seed;
    let bytes=serde_json::to_vec(&json!({"kind":"stop","command_id":id(),"run_id":seed.run_spec.run_id,"idempotency_key":key,"payload":{"reason":"Requested stop"}})).unwrap();
    let result = store
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
    (bytes, result)
}
async fn host(
    store: Arc<Mutex<LocalStore>>,
    p: &PreparedFixtureTurn,
    profile: &FixtureProfile,
) -> FixtureHost {
    let seed = store
        .lock()
        .await
        .read_local_turn(&p.workspace, &p.actor, &p.turn)
        .await
        .unwrap()
        .seed;
    FixtureHost {
        store,
        actor: p.actor.clone(),
        seed,
        profile: profile.clone(),
        compatibility: hash(&record_bytes(profile).unwrap()),
        active: None,
        active_ordinal: 0,
    }
}
#[test]
fn restart_with_pending_model_and_stop_returns_blocked_cancellation_without_reissue() {
    run(|| async {
        let root = Root::new();
        let (store, p, profile) = setup(&root).await;
        let mut host = host(store.clone(), &p, &profile).await;
        let seed = host.seed.clone();
        let mut state = host.restore(&seed).await.unwrap().state;
        let step = host.seal_step(&seed, &state).await.unwrap();
        state.steps_started = 1;
        assert!(matches!(
            host.model_record(&seed, &step, &state).await.unwrap(),
            ModelRecord::Fresh
        ));
        let model = step.model_call_id.clone();
        drop(host);
        let (bytes, receipt) = stop(&store, &p, "stop-pending-model").await;
        close(store).await;
        let store = Arc::new(Mutex::new(LocalStore::open(&root.0).await.unwrap()));
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
            matches!(&outcome,TurnOutcome::Cancelled {unresolved_receipt_refs,..} if unresolved_receipt_refs.as_slice()==[model.clone()])
        );
        let snap = store
            .lock()
            .await
            .reconcile_local_execution(&p.workspace, &p.actor, &p.turn)
            .await
            .unwrap();
        assert_eq!(snap.unresolved_model_calls, vec![model]);
        assert!(snap.unresolved_invocations.is_empty());
        let before = store.lock().await.diagnostic_bytes().await.unwrap();
        let replay = execute_fixture_turn(
            store.clone(),
            p.workspace.clone(),
            p.actor.clone(),
            p.turn.clone(),
            profile,
            EngineConfig::default(),
        )
        .await
        .unwrap();
        assert_eq!(replay, outcome);
        let sample = clock::sample().unwrap();
        let retry = store
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
        assert_eq!(retry, receipt);
        assert_eq!(store.lock().await.diagnostic_bytes().await.unwrap(), before);
        let recovery = store
            .lock()
            .await
            .recover_local_controls(&p.workspace, &p.actor)
            .await
            .unwrap();
        assert_eq!(
            recovery.runs[0].state,
            avencrew_contracts::wire::RunState::Blocked
        );
        assert_eq!(
            recovery.runs[0].gate,
            avencrew_store_sqlite::RecoveryGate::CancellationPending
        );
        close(store).await;
    });
}

struct StopBeforeClaim<'a> {
    host: &'a mut FixtureHost,
    p: &'a PreparedFixtureTurn,
    executed: usize,
}
impl broker::Host for StopBeforeClaim<'_> {
    async fn commit_proposals(
        &mut self,
        s: &StepBinding,
        p: &[PreparedInvocation],
    ) -> Result<(), BrokerError> {
        self.host.commit_proposals(s, p).await
    }
    async fn committed_digest(
        &mut self,
        p: &PreparedInvocation,
    ) -> Result<Option<Digest>, BrokerError> {
        self.host.committed_digest(p).await
    }
    async fn current_policy(
        &mut self,
        p: &PreparedInvocation,
    ) -> Result<PolicySnapshot, BrokerError> {
        self.host.current_policy(p).await
    }
    async fn claim_dispatch(
        &mut self,
        p: &PreparedInvocation,
        a: &DispatchAdmission,
    ) -> Result<Dispatch, BrokerError> {
        // Real command acceptance occurs after Broker has authorized the exposed
        // invocation, before the actual journal dispatch claim.
        stop(&self.host.store, self.p, "stop-at-claim").await;
        self.host.claim_dispatch(p, a).await
    }
    async fn execute(&mut self, e: &Execution<'_>) -> Result<ToolResult, BrokerError> {
        self.executed += 1;
        self.host.execute(e).await
    }
    async fn record_result(
        &mut self,
        e: &Execution<'_>,
        r: &ToolResult,
    ) -> Result<(), BrokerError> {
        self.host.record_result(e, r).await
    }
    async fn record_unknown(&mut self, e: &Execution<'_>) -> Result<(), BrokerError> {
        self.host.record_unknown(e).await
    }
}
#[test]
fn accepted_stop_between_broker_policy_and_dispatch_claim_executes_nothing() {
    run(|| async {
        let root = Root::new();
        let (store, p, profile) = setup(&root).await;
        let mut host = host(store.clone(), &p, &profile).await;
        let seed = host.seed.clone();
        let mut state = host.restore(&seed).await.unwrap().state;
        let step = host.seal_step(&seed, &state).await.unwrap();
        state.steps_started = 1;
        host.model_record(&seed, &step, &state).await.unwrap();
        // Pure fixture normalization supplies a whole retained response; authority
        // and invocation identities still come exclusively from the real Host.
        let mut input = profile.episodes[0].input.clone();
        input.seed.model_call_id = step.model_call_id.clone();
        input.seed.step_id = step.broker.step().step_id.clone();
        input.seed.provider_profile_id = step.provider_profile_id.clone();
        input.seed.compatibility_digest = step.compatibility_digest.clone();
        input.seed.bindings = vec![FixtureBinding {
            name: "fixture.read".into(),
            binding_id: fixed(18),
        }];
        input.seed.invocation_slots = input.seed.invocation_slots.iter().map(|_| id()).collect();
        let completion = normalize_fixture(&input).unwrap();
        host.record_model(&seed, &step, &Ok(completion.clone()), &state)
            .await
            .unwrap();
        let proposals =
            commit_fixture_proposals(&step.broker, &mut host, &step.model_call_id, &completion)
                .await
                .unwrap();
        let p0 = &proposals[0];
        let request = Invocation {
            invocation_id: p0.invocation_id().clone(),
            binding_id: p0.context().binding_id.clone(),
            arguments: p0.arguments().clone(),
            origin: p0.origin().clone(),
        };
        {
            let mut racing = StopBeforeClaim {
                host: &mut host,
                p: &p,
                executed: 0,
            };
            assert!(matches!(
                step.broker.invoke(&mut racing, request).await,
                Err(BrokerError::Cancelled | BrokerError::StaleAuthority)
            ));
            assert_eq!(racing.executed, 0);
        }
        drop(host);
        let budget = store
            .lock()
            .await
            .local_execution_budget(&p.workspace, &p.actor, &p.turn)
            .await
            .unwrap();
        assert_eq!(budget.dispatches.value(), 0);
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
        assert!(matches!(outcome, TurnOutcome::Cancelled { .. }));
        let before = store.lock().await.diagnostic_bytes().await.unwrap();
        let (_, receipt) = stop(&store, &p, "stop-after-terminal").await;
        assert!(matches!(
            receipt,
            avencrew_contracts::wire::CommandReceipt::Terminal { .. }
        ));
        assert_eq!(store.lock().await.diagnostic_bytes().await.unwrap(), before);
        close(store).await;
    });
}
#[test]
fn late_stop_is_rejected_and_candidate_replay_does_not_start_more_work() {
    run(|| async {
        let root = Root::new();
        let (store, p, profile) = setup(&root).await;
        let saved = execute_fixture_turn(
            store.clone(),
            p.workspace.clone(),
            p.actor.clone(),
            p.turn.clone(),
            profile.clone(),
            EngineConfig::default(),
        )
        .await
        .unwrap();
        let before = store.lock().await.diagnostic_bytes().await.unwrap();
        let seed = store
            .lock()
            .await
            .read_local_turn(&p.workspace, &p.actor, &p.turn)
            .await
            .unwrap()
            .seed;
        for kind in ["stop", "pause", "steer"] {
            let version = store
                .lock()
                .await
                .recover_local_controls(&p.workspace, &p.actor)
                .await
                .unwrap()
                .runs[0]
                .version
                .clone();
            let payload = match kind {
                "stop" => json!({"reason":"late stop"}),
                "pause" => json!({"reason":"late pause", "expected_revision":version}),
                _ => json!({"text":"late input", "expected_revision":version}),
            };
            let bytes = serde_json::to_vec(&json!({"kind":kind,"command_id":id(),"run_id":seed.run_spec.run_id,"idempotency_key":format!("after-candidate-{kind}"),"payload":payload})).unwrap();
            assert!(matches!(
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
                        at: &clock::sample().unwrap().now,
                    })
                    .await,
                Err(avencrew_store_sqlite::ControlError::Conflict)
            ));
        }
        assert_eq!(store.lock().await.diagnostic_bytes().await.unwrap(), before);
        close(store).await;
        let store = Arc::new(Mutex::new(LocalStore::open(&root.0).await.unwrap()));
        let before = store.lock().await.diagnostic_bytes().await.unwrap();
        assert_eq!(
            execute_fixture_turn(
                store.clone(),
                p.workspace.clone(),
                p.actor.clone(),
                p.turn.clone(),
                profile,
                EngineConfig::default()
            )
            .await
            .unwrap(),
            saved
        );
        let snap = store
            .lock()
            .await
            .reconcile_local_execution(&p.workspace, &p.actor, &p.turn)
            .await
            .unwrap();
        assert_eq!(snap.progress.outcome, Some(saved));
        assert!(!snap.cancellation_requested);
        assert_eq!(store.lock().await.diagnostic_bytes().await.unwrap(), before);
        close(store).await;
    });
}

#[test]
fn unsupported_recovery_is_a_held_engine_error() {
    assert_eq!(storage(TurnError::Unsupported), EngineError::Unsupported);
    assert_eq!(storage(TurnError::Conflict), EngineError::Storage);
}
