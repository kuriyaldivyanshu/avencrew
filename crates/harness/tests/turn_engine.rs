//! Engine + fixture adapter + broker consumer. Commit cuts use the shared
//! in-memory host; no SQLite durability or production authority is claimed.
mod support;
use avencrew_broker::{self as broker, *};
use avencrew_contracts::{scalars::*, wire::*};
use avencrew_harness::{
    engine::{self, *},
    model::fixture::*,
};
use serde_json::json;
use std::collections::BTreeMap;
use support::*;

fn seed() -> RpcRequestHarnessStartTurnBody {
    let (_, host) = setup();
    serde_json::from_value(json!({
        "run_spec": {"task_id":id(4),"task_revision_id":id(5),"run_id":id(100),
        "session_id":id(7),"branch_id":id(8),"workspace_id":id(1),"project_id":id(6),
        "worker_id":null,"responsibility_id":null,"requester_id":id(3),"owner_id":id(3),
        "acting_principal_id":id(3),"audience_policy_id":id(9),"execution_policy_id":id(10),
        "objective_version_id":id(11),"check_refs":[],"instruction_digests":[],
        "model_profile_id":id(19),"environment_recipe_version_id":null,"budget_period_refs":[],
        "deadline":null,"placement":"local","accepted_input_seq":"0"},
        "lease":host.policy.lease,"turn_id":id(12),"accepted_input_cut":"0","checkpoint_id":null,
        "limits":{"model_steps":5,"active_wall_ms":10000,"output_bytes":32768}
    }))
    .unwrap()
}
fn final_response() -> ModelCompletion {
    let (_, mut result) = fixture("complete_split_arguments");
    result.response.finish = NormalizedModelResponseFinish::Final;
    result.response.items = List::new(vec![NormalizedModelResponseItemsItem::Text {
        text: Text::new("done; candidate only").unwrap(),
    }])
    .unwrap();
    result.proposal_identities.clear();
    Ok(result)
}
fn tool_response() -> ModelCompletion {
    Ok(fixture("complete_split_arguments").1)
}
fn execute(work: impl std::future::Future<Output = ()>) {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap()
        .block_on(work);
}
struct EngineFixture {
    tools: FixtureHost,
    state: TurnState,
    scripts: Vec<ModelCompletion>,
    records: BTreeMap<DomainId, Option<ModelCompletion>>,
    pending: Option<u32>,
    outcome: Option<TurnOutcome>,
    trace: Vec<&'static str>,
    observations: Vec<DomainId>,
    generated: usize,
    controls_count: usize,
    stop_at: usize,
    pause: bool,
    steering_at: usize,
    same_progress: bool,
    fail_at: Option<&'static str>,
    lost_model_reply: bool,
    block_model: bool,
    block_tool: bool,
    context_bytes: u32,
    wrong_scope: bool,
    wrong_lease: bool,
}
impl EngineFixture {
    fn new(scripts: Vec<ModelCompletion>) -> Self {
        Self {
            tools: setup().1,
            state: TurnState {
                steps_started: 0,
                no_progress_steps: 0,
                active_elapsed_ms: 0,
                applied_input_cut: Counter::new("0").unwrap(),
                progress_digest: digest(),
            },
            scripts,
            records: BTreeMap::new(),
            pending: None,
            outcome: None,
            trace: vec![],
            observations: vec![],
            generated: 0,
            controls_count: 0,
            stop_at: usize::MAX,
            pause: false,
            steering_at: usize::MAX,
            same_progress: false,
            fail_at: None,
            lost_model_reply: false,
            block_model: false,
            block_tool: false,
            context_bytes: 100,
            wrong_scope: false,
            wrong_lease: false,
        }
    }
    fn cut(&mut self, name: &'static str) -> Result<(), EngineError> {
        self.trace.push(name);
        if self.fail_at == Some(name) {
            Err(EngineError::Storage)
        } else {
            Ok(())
        }
    }
    fn step(&mut self, ordinal: u32) -> ModelStep {
        let (base, _) = setup();
        let mut step = base.step().clone();
        step.step_id = id(17 + ordinal as usize * 100);
        let mut context = base.context().clone();
        context.step_id = step.step_id.clone();
        if self.wrong_scope {
            context.run_id = id(999);
        }
        let mut lease = self.tools.policy.lease.clone();
        lease.run_id = context.run_id.clone();
        if self.wrong_lease {
            lease.authority = ExecutionAuthority::Server;
        }
        let catalog = SealedCatalog::seal(vec![binding(18, "workspace_read")]).unwrap();
        self.tools.policy.context = context.clone();
        self.tools.policy.model_call_id = Some(id(16 + ordinal as usize * 100));
        self.tools.steps.insert(step.step_id.clone(), step.clone());
        ModelStep {
            broker: Broker::new(step, context, lease, catalog).unwrap(),
            model_call_id: id(16 + ordinal as usize * 100),
            request_digest: digest(),
            provider_profile_id: id(19),
            compatibility_digest: digest(),
            context_bytes: self.context_bytes,
        }
    }
}
impl broker::Host for EngineFixture {
    async fn commit_proposals(
        &mut self,
        step: &StepBinding,
        proposals: &[PreparedInvocation],
    ) -> Result<(), BrokerError> {
        self.trace.push("proposals");
        assert!(self.records.values().any(
            |record| matches!(record,Some(Ok(result)) if result.response.step_id==step.step_id)
        ));
        broker::Host::commit_proposals(&mut self.tools, step, proposals).await
    }
    async fn committed_digest(
        &mut self,
        invocation: &PreparedInvocation,
    ) -> Result<Option<Digest>, BrokerError> {
        broker::Host::committed_digest(&mut self.tools, invocation).await
    }
    async fn current_policy(
        &mut self,
        invocation: &PreparedInvocation,
    ) -> Result<PolicySnapshot, BrokerError> {
        broker::Host::current_policy(&mut self.tools, invocation).await
    }
    async fn claim_dispatch(
        &mut self,
        invocation: &PreparedInvocation,
        admission: &DispatchAdmission,
    ) -> Result<Dispatch, BrokerError> {
        self.trace.push("dispatch");
        broker::Host::claim_dispatch(&mut self.tools, invocation, admission).await
    }
    async fn execute(&mut self, proof: &Execution<'_>) -> Result<ToolResult, BrokerError> {
        let result = broker::Host::execute(&mut self.tools, proof).await;
        if self.block_tool {
            std::future::pending::<()>().await;
        }
        result
    }
    async fn record_result(
        &mut self,
        proof: &Execution<'_>,
        result: &ToolResult,
    ) -> Result<(), BrokerError> {
        self.trace.push("result");
        broker::Host::record_result(&mut self.tools, proof, result).await
    }
    async fn record_unknown(&mut self, proof: &Execution<'_>) -> Result<(), BrokerError> {
        broker::Host::record_unknown(&mut self.tools, proof).await
    }
}
impl engine::Host for EngineFixture {
    async fn restore(
        &mut self,
        _: &RpcRequestHarnessStartTurnBody,
    ) -> Result<Restoration, EngineError> {
        self.cut("restore")?;
        Ok(Restoration {
            state: self.state.clone(),
            pending_step: self.pending.map(|ordinal| self.step(ordinal)),
            outcome: self.outcome.clone(),
        })
    }
    async fn controls(
        &mut self,
        _: &RpcRequestHarnessStartTurnBody,
        state: &TurnState,
    ) -> Result<Control, EngineError> {
        self.cut("controls")?;
        self.controls_count += 1;
        if self.controls_count >= self.stop_at {
            return Ok(Control::Stop {
                applied_input_cut: state.applied_input_cut.clone(),
                cleanup_refs: List::new(vec![]).unwrap(),
                unresolved_receipt_refs: List::new(vec![]).unwrap(),
            });
        }
        if self.pause {
            return Ok(Control::Pause {
                applied_input_cut: state.applied_input_cut.clone(),
            });
        }
        Ok(Control::Continue {
            applied_input_cut: if self.controls_count >= self.steering_at {
                Counter::new("1").unwrap()
            } else {
                state.applied_input_cut.clone()
            },
        })
    }
    async fn seal_step(
        &mut self,
        _: &RpcRequestHarnessStartTurnBody,
        state: &TurnState,
    ) -> Result<ModelStep, EngineError> {
        self.cut("seal")?;
        Ok(self.step(state.steps_started + 1))
    }
    async fn model_record(
        &mut self,
        _: &RpcRequestHarnessStartTurnBody,
        step: &ModelStep,
        state: &TurnState,
    ) -> Result<ModelRecord, EngineError> {
        self.cut("intent")?;
        if let Some(record) = self.records.get(&step.model_call_id) {
            return Ok(match record {
                Some(result) => ModelRecord::Response(Box::new(result.clone())),
                None => ModelRecord::Unresolved,
            });
        }
        self.state = state.clone();
        self.pending = Some(state.steps_started);
        self.records.insert(step.model_call_id.clone(), None);
        Ok(ModelRecord::Fresh)
    }
    async fn generate(&mut self, proof: &Generation<'_>) -> ModelCompletion {
        self.trace.push("generate");
        self.generated += 1;
        assert!(self.records.contains_key(&proof.step().model_call_id));
        if self.block_model {
            std::future::pending::<()>().await;
        }
        let mut result = self.scripts[self.state.steps_started as usize - 1].clone();
        if let Ok(result) = &mut result {
            result.response.model_call_id = proof.step().model_call_id.clone();
            result.response.step_id = proof.step().broker.step().step_id.clone();
            for (n, identity) in result.proposal_identities.iter_mut().enumerate() {
                identity.invocation_id = id(300 + self.state.steps_started as usize * 100 + n);
            }
        }
        result
    }
    async fn record_model(
        &mut self,
        _: &RpcRequestHarnessStartTurnBody,
        step: &ModelStep,
        result: &ModelCompletion,
        state: &TurnState,
    ) -> Result<(), EngineError> {
        self.cut("response")?;
        self.records
            .insert(step.model_call_id.clone(), Some(result.clone()));
        self.state = state.clone();
        if self.lost_model_reply {
            return Err(EngineError::UnknownOutcome);
        }
        Ok(())
    }
    async fn observe(
        &mut self,
        _: &RpcRequestHarnessStartTurnBody,
        _: &ModelStep,
        observations: &[Observation],
        state: &TurnState,
    ) -> Result<Digest, EngineError> {
        self.cut("observe")?;
        for observation in observations {
            if !self.observations.contains(&observation.invocation_id) {
                self.observations.push(observation.invocation_id.clone());
            }
        }
        let progress = if self.same_progress {
            state.progress_digest.clone()
        } else {
            Digest::new(format!("{:064x}", state.steps_started)).unwrap()
        };
        self.state = state.clone();
        self.state.no_progress_steps = if progress == state.progress_digest {
            state.no_progress_steps + 1
        } else {
            0
        };
        self.state.progress_digest = progress.clone();
        self.pending = None;
        Ok(progress)
    }
    async fn stage_candidate(
        &mut self,
        _: &RpcRequestHarnessStartTurnBody,
        _: &ModelStep,
        _: &FixtureResult,
    ) -> Result<Candidate, EngineError> {
        self.cut("candidate")?;
        Ok(Candidate {
            artifacts: List::new(vec![]).unwrap(),
            checks: List::new(vec![]).unwrap(),
        })
    }
    async fn checkpoint(
        &mut self,
        _: &RpcRequestHarnessStartTurnBody,
        state: &TurnState,
    ) -> Result<DomainId, EngineError> {
        self.cut("checkpoint")?;
        self.state = state.clone();
        Ok(id(600))
    }
    async fn commit_outcome(
        &mut self,
        _: &RpcRequestHarnessStartTurnBody,
        state: &TurnState,
        outcome: &TurnOutcome,
    ) -> Result<(), EngineError> {
        self.cut("outcome")?;
        self.state = state.clone();
        self.outcome = Some(outcome.clone());
        self.pending = None;
        Ok(())
    }
}

#[test]
fn complete_turn_has_intent_response_proposals_ordered_results_and_candidate_only() {
    execute(async {
        let mut host = EngineFixture::new(vec![tool_response(), final_response()]);
        let outcome = run_turn(&mut host, &seed(), EngineConfig::default())
            .await
            .unwrap();
        assert!(
            matches!(outcome,TurnOutcome::CandidateResult {objective_version_id,..} if objective_version_id==id(11))
        );
        assert_eq!(host.generated, 2);
        assert_eq!(host.tools.dispatch_count, 1);
        let position = |name| host.trace.iter().position(|entry| *entry == name).unwrap();
        assert!(
            position("intent") < position("generate")
                && position("generate") < position("response")
                && position("response") < position("proposals")
                && position("proposals") < position("dispatch")
                && position("result") < position("observe")
                && position("candidate") < position("checkpoint")
                && position("checkpoint") < position("outcome")
        );
        let before = (host.generated, host.tools.dispatch_count, host.trace.len());
        let replay = run_turn(&mut host, &seed(), EngineConfig::default())
            .await
            .unwrap();
        assert_eq!(host.outcome.as_ref(), Some(&replay));
        assert_eq!(
            (host.generated, host.tools.dispatch_count),
            (before.0, before.1)
        );
        assert_eq!(&host.trace[before.2..], &["restore"]);
    });
}
#[test]
fn every_commit_cut_prevents_unacknowledged_follow_on_work() {
    execute(async {
        for cut in [
            "restore",
            "controls",
            "seal",
            "intent",
            "response",
            "observe",
            "candidate",
            "checkpoint",
            "outcome",
        ] {
            let scripts = if cut == "candidate" {
                vec![final_response()]
            } else {
                vec![tool_response(), final_response()]
            };
            let mut host = EngineFixture::new(scripts);
            host.fail_at = Some(cut);
            assert_eq!(
                run_turn(&mut host, &seed(), EngineConfig::default())
                    .await
                    .err(),
                Some(EngineError::Storage),
                "{cut}"
            );
            assert!(host.outcome.is_none());
            if ["restore", "controls", "seal", "intent", "response"].contains(&cut) {
                assert_eq!(host.tools.dispatch_count, 0, "{cut}");
            }
            if ["restore", "controls", "seal", "intent"].contains(&cut) {
                assert_eq!(host.generated, 0, "{cut}");
            }
        }
    });
}
#[test]
fn retained_response_resumes_without_duplicate_inference_or_tool() {
    execute(async {
        let mut host = EngineFixture::new(vec![tool_response(), final_response()]);
        host.lost_model_reply = true;
        assert_eq!(
            run_turn(&mut host, &seed(), EngineConfig::default())
                .await
                .err(),
            Some(EngineError::UnknownOutcome)
        );
        assert_eq!(host.generated, 1);
        assert_eq!(host.tools.dispatch_count, 0);
        host.lost_model_reply = false;
        assert!(matches!(
            run_turn(&mut host, &seed(), EngineConfig::default())
                .await
                .unwrap(),
            TurnOutcome::CandidateResult { .. }
        ));
        assert_eq!(host.generated, 2);
        assert_eq!(host.tools.dispatch_count, 1);
        let mut host = EngineFixture::new(vec![tool_response(), final_response()]);
        host.fail_at = Some("observe");
        assert!(run_turn(&mut host, &seed(), EngineConfig::default())
            .await
            .is_err());
        assert_eq!(host.tools.dispatch_count, 1);
        host.fail_at = None;
        run_turn(&mut host, &seed(), EngineConfig::default())
            .await
            .unwrap();
        assert_eq!(host.tools.dispatch_count, 1);
        assert_eq!(host.generated, 2);
    });
}
#[test]
fn unresolved_model_intent_blocks_inference_and_tool_work() {
    execute(async {
        let mut host = EngineFixture::new(vec![]);
        host.state.steps_started = 1;
        host.pending = Some(1);
        host.records.insert(id(116), None);
        assert!(matches!(
            run_turn(&mut host, &seed(), EngineConfig::default())
                .await
                .unwrap(),
            TurnOutcome::Failed {
                retry_class: TurnOutcomeFailedRetryClass::ReconcileFirst,
                ..
            }
        ));
        assert_eq!(host.generated, 0);
        assert_eq!(host.tools.dispatch_count, 0);
    });
}
#[test]
fn stop_pause_and_new_steering_are_honored_at_inference_and_dispatch_boundaries() {
    execute(async {
        for stop in [1, 2, 3] {
            let mut host = EngineFixture::new(vec![tool_response()]);
            host.stop_at = stop;
            assert!(matches!(
                run_turn(&mut host, &seed(), EngineConfig::default())
                    .await
                    .unwrap(),
                TurnOutcome::Cancelled { .. }
            ));
            assert_eq!(host.tools.dispatch_count, 0);
            assert_eq!(host.generated, usize::from(stop > 1));
        }
        let mut host = EngineFixture::new(vec![]);
        host.pause = true;
        assert!(matches!(
            run_turn(&mut host, &seed(), EngineConfig::default())
                .await
                .unwrap(),
            TurnOutcome::Paused { .. }
        ));
        let mut host = EngineFixture::new(vec![tool_response()]);
        host.steering_at = 2;
        assert!(matches!(
            run_turn(&mut host, &seed(), EngineConfig::default())
                .await
                .unwrap(),
            TurnOutcome::Yielded {
                reason: TurnOutcomeYieldedReason::User,
                ..
            }
        ));
        assert_eq!(host.tools.dispatch_count, 0);
    });
}
#[test]
fn step_context_time_and_no_progress_limits_are_retained() {
    execute(async {
        let mut limited = seed();
        limited.limits.model_steps = SmallInteger::new(1).unwrap();
        let mut host = EngineFixture::new(vec![tool_response()]);
        assert!(matches!(
            run_turn(&mut host, &limited, EngineConfig::default())
                .await
                .unwrap(),
            TurnOutcome::Yielded {
                reason: TurnOutcomeYieldedReason::TurnLimit,
                ..
            }
        ));
        assert_eq!(host.generated, 1);
        let mut host = EngineFixture::new(vec![]);
        host.context_bytes = 1_048_576;
        assert!(matches!(
            run_turn(&mut host, &seed(), EngineConfig::default())
                .await
                .unwrap(),
            TurnOutcome::Yielded {
                reason: TurnOutcomeYieldedReason::ContextLimit,
                ..
            }
        ));
        assert_eq!(host.generated, 0);
        let mut host = EngineFixture::new(vec![]);
        host.state.active_elapsed_ms = 10000;
        assert!(matches!(
            run_turn(&mut host, &seed(), EngineConfig::default())
                .await
                .unwrap(),
            TurnOutcome::Yielded {
                reason: TurnOutcomeYieldedReason::TurnLimit,
                ..
            }
        ));
        assert_eq!(host.generated, 0);
        let mut host = EngineFixture::new(vec![tool_response(); 3]);
        host.same_progress = true;
        assert!(matches!(
            run_turn(&mut host, &seed(), EngineConfig::default())
                .await
                .unwrap(),
            TurnOutcome::Failed {
                retry_class: TurnOutcomeFailedRetryClass::Remediation,
                ..
            }
        ));
        assert_eq!(host.generated, 3);
        assert_eq!(host.state.no_progress_steps, 3);
    });
}
#[test]
fn interrupted_failed_refused_length_and_malformed_results_do_not_dispatch() {
    execute(async {
        for finish in [
            NormalizedModelResponseFinish::Interrupted,
            NormalizedModelResponseFinish::Failed,
            NormalizedModelResponseFinish::Refused,
            NormalizedModelResponseFinish::Length,
        ] {
            let Ok(mut result) = final_response() else {
                unreachable!()
            };
            result.response.finish = finish;
            let mut host = EngineFixture::new(vec![Ok(result)]);
            let outcome = run_turn(&mut host, &seed(), EngineConfig::default())
                .await
                .unwrap();
            assert!(!matches!(outcome, TurnOutcome::CandidateResult { .. }));
            assert_eq!(host.tools.dispatch_count, 0);
        }
        let mut host = EngineFixture::new(vec![Err(FixtureFailure {
            code: FixtureError::InvalidArguments,
            usage: FixtureUsage::Unknown,
        })]);
        assert!(matches!(
            run_turn(&mut host, &seed(), EngineConfig::default())
                .await
                .unwrap(),
            TurnOutcome::Failed { .. }
        ));
        assert_eq!(host.tools.dispatch_count, 0);
        let mut host = EngineFixture::new(vec![final_response()]);
        host.wrong_scope = true;
        assert_eq!(
            run_turn(&mut host, &seed(), EngineConfig::default())
                .await
                .err(),
            Some(EngineError::StaleAuthority)
        );
        assert_eq!(host.generated, 0);
    });
}
#[test]
fn model_and_tool_timeouts_preserve_unknown_without_retry() {
    execute(async {
        let mut limited = seed();
        limited.limits.active_wall_ms = SmallInteger::new(100).unwrap();
        let mut host = EngineFixture::new(vec![]);
        host.block_model = true;
        let outcome = run_turn(&mut host, &limited, EngineConfig::default())
            .await
            .unwrap();
        assert!(matches!(
            outcome,
            TurnOutcome::Failed {
                retry_class: TurnOutcomeFailedRetryClass::ReconcileFirst,
                ..
            }
        ));
        assert_eq!(host.generated, 1);
        assert_eq!(host.tools.dispatch_count, 0);
        assert!(matches!(
            host.records.values().next().unwrap(),
            Some(Err(FixtureFailure {
                usage: FixtureUsage::Unknown,
                ..
            }))
        ));
        let mut host = EngineFixture::new(vec![tool_response()]);
        host.block_tool = true;
        let outcome = run_turn(&mut host, &limited, EngineConfig::default())
            .await
            .unwrap();
        assert!(matches!(
            outcome,
            TurnOutcome::Failed {
                retry_class: TurnOutcomeFailedRetryClass::ReconcileFirst,
                ..
            }
        ));
        assert_eq!(host.tools.dispatch_count, 1);
        assert!(host
            .tools
            .rows
            .values()
            .any(|(_, state)| matches!(state, Retained::Started)));
    });
}

fn two_tool_response() -> ModelCompletion {
    let Ok(mut result) = tool_response() else {
        unreachable!()
    };
    let mut second = result.response.items.as_slice()[0].clone();
    let NormalizedModelResponseItemsItem::ToolProposal {
        provider_call_id, ..
    } = &mut second
    else {
        unreachable!()
    };
    *provider_call_id = Text::new("second-provider-call").unwrap();
    let mut identity = result.proposal_identities[0].clone();
    identity.item = 1;
    identity.provider_call_id = "second-provider-call".into();
    result.proposal_identities.push(identity);
    result.response.items =
        List::new(vec![result.response.items.as_slice()[0].clone(), second]).unwrap();
    Ok(result)
}
#[test]
fn provider_order_and_stop_between_calls_preserve_committed_prefix() {
    execute(async {
        let mut host = EngineFixture::new(vec![two_tool_response(), final_response()]);
        run_turn(&mut host, &seed(), EngineConfig::default())
            .await
            .unwrap();
        assert_eq!(host.observations, vec![id(400), id(401)]);
        assert_eq!(host.tools.dispatch_count, 2);
        let mut host = EngineFixture::new(vec![two_tool_response()]);
        host.stop_at = 4;
        assert!(matches!(
            run_turn(&mut host, &seed(), EngineConfig::default())
                .await
                .unwrap(),
            TurnOutcome::Cancelled { .. }
        ));
        assert_eq!(host.tools.dispatch_count, 1);
        assert!(matches!(host.tools.rows[&id(400)].1, Retained::Result(_)));
        assert!(matches!(host.tools.rows[&id(401)].1, Retained::Proposed));
    });
}
#[test]
fn response_bounds_malformed_proposals_and_opaque_compatibility_fail_closed() {
    execute(async {
        let Ok(mut result) = final_response() else {
            unreachable!()
        };
        result.response.items = List::new(vec![NormalizedModelResponseItemsItem::Text {
            text: Text::new("x".repeat(2000)).unwrap(),
        }])
        .unwrap();
        let mut limited = seed();
        limited.limits.output_bytes = SmallInteger::new(1000).unwrap();
        let mut host = EngineFixture::new(vec![Ok(result)]);
        assert!(matches!(
            run_turn(&mut host, &limited, EngineConfig::default())
                .await
                .unwrap(),
            TurnOutcome::Failed { .. }
        ));
        assert!(!host.trace.contains(&"candidate"));
        for mismatch in [true, false] {
            let Ok(mut result) = final_response() else {
                unreachable!()
            };
            result.response.items =
                List::new(vec![NormalizedModelResponseItemsItem::OpaqueContinuation {
                    provider_profile_id: if mismatch { id(999) } else { id(19) },
                    compatibility_digest: if mismatch {
                        digest()
                    } else {
                        Digest::new("1".repeat(64)).unwrap()
                    },
                    blob_ref: id(800),
                }])
                .unwrap();
            let mut host = EngineFixture::new(vec![Ok(result)]);
            assert!(matches!(
                run_turn(&mut host, &seed(), EngineConfig::default())
                    .await
                    .unwrap(),
                TurnOutcome::Failed { .. }
            ));
            assert!(!host.trace.contains(&"candidate"));
        }
        let Ok(mut result) = tool_response() else {
            unreachable!()
        };
        result.proposal_identities[0].item = 99;
        let mut host = EngineFixture::new(vec![Ok(result)]);
        assert!(matches!(
            run_turn(&mut host, &seed(), EngineConfig::default())
                .await
                .unwrap(),
            TurnOutcome::Failed { .. }
        ));
        assert_eq!(host.tools.dispatch_count, 0);
    });
}
#[test]
fn current_denial_unknown_result_and_post_staging_stop_never_continue() {
    execute(async {
        let mut host = EngineFixture::new(vec![two_tool_response()]);
        host.tools.policy.task_allowed = false;
        assert!(matches!(
            run_turn(&mut host, &seed(), EngineConfig::default())
                .await
                .unwrap(),
            TurnOutcome::Failed {
                retry_class: TurnOutcomeFailedRetryClass::Remediation,
                ..
            }
        ));
        assert_eq!(host.generated, 1);
        assert_eq!(host.tools.dispatch_count, 0);
        let mut host = EngineFixture::new(vec![two_tool_response()]);
        host.tools.fail_claim_reply = true;
        assert!(matches!(
            run_turn(&mut host, &seed(), EngineConfig::default())
                .await
                .unwrap(),
            TurnOutcome::Failed {
                retry_class: TurnOutcomeFailedRetryClass::ReconcileFirst,
                ..
            }
        ));
        assert_eq!(host.generated, 1);
        assert_eq!(host.tools.dispatch_count, 0);
        assert!(matches!(host.tools.rows[&id(400)].1, Retained::Started));
        assert!(matches!(host.tools.rows[&id(401)].1, Retained::Proposed));
        let mut host = EngineFixture::new(vec![final_response()]);
        host.stop_at = 3;
        assert!(matches!(
            run_turn(&mut host, &seed(), EngineConfig::default())
                .await
                .unwrap(),
            TurnOutcome::Cancelled { .. }
        ));
        assert!(host.trace.contains(&"candidate"));
        assert_eq!(host.tools.dispatch_count, 0);
    });
}

#[test]
fn accepted_inputs_are_applied_before_first_step_and_invalid_restores_fail_closed() {
    execute(async {
        let mut accepted = seed();
        accepted.accepted_input_cut = Counter::new("1").unwrap();
        let mut host = EngineFixture::new(vec![final_response()]);
        host.steering_at = 1;
        assert!(matches!(
            run_turn(&mut host, &accepted, EngineConfig::default())
                .await
                .unwrap(),
            TurnOutcome::CandidateResult { .. }
        ));
        assert_eq!(host.state.applied_input_cut.value(), 1);
        let mut host = EngineFixture::new(vec![]);
        host.wrong_lease = true;
        assert_eq!(
            run_turn(&mut host, &seed(), EngineConfig::default())
                .await
                .err(),
            Some(EngineError::StaleAuthority)
        );
        assert_eq!(host.generated, 0);
        let mut host = EngineFixture::new(vec![]);
        assert_eq!(
            run_turn(&mut host, &accepted, EngineConfig::default())
                .await
                .err(),
            Some(EngineError::InvalidState)
        );
        assert_eq!(host.generated, 0);
        for case in 0..4 {
            let mut host = EngineFixture::new(vec![]);
            let mut request = seed();
            match case {
                0 => host.state.steps_started = 51,
                1 => host.pending = Some(1),
                2 => request.lease.run_id = id(999),
                _ => host.state.no_progress_steps = 51,
            }
            assert_eq!(
                run_turn(&mut host, &request, EngineConfig::default())
                    .await
                    .err(),
                Some(EngineError::InvalidState)
            );
            assert_eq!(host.generated, 0);
        }
        let mut host = EngineFixture::new(vec![two_tool_response()]);
        host.tools.result_override=Some(serde_json::from_value(json!({
        "status":"unknown","invocation_id":id(400),"schema_version":"1.0","artifact_refs":[],
        "receipt_ref":null,"truncated":false,"next_cursor":null,"usage_refs":[],"data":null,
        "error":{"code":"UNKNOWN_OUTCOME","message":"uncertain read","evidence_refs":[]}
    })).unwrap());
        assert!(matches!(
            run_turn(&mut host, &seed(), EngineConfig::default())
                .await
                .unwrap(),
            TurnOutcome::Failed {
                retry_class: TurnOutcomeFailedRetryClass::ReconcileFirst,
                ..
            }
        ));
        assert_eq!(host.tools.dispatch_count, 1);
        assert!(matches!(host.tools.rows[&id(401)].1, Retained::Proposed));
    });
}
