//! SQLite-backed coordinator for the explicitly bounded synthetic fixture profile.
//! One harness loop; no provider, host shell, guest or external action is enabled.
mod clock;
mod launch;
use avencrew_broker::{
    self as broker, Binding, Broker, BrokerError, Dispatch, DispatchAdmission, Effect, Execution,
    PolicySnapshot, PreparedInvocation, SealedCatalog,
};
use avencrew_contracts::{
    runtime::{record_bytes, LocalStep, ModelCompletion},
    scalars::{Counter, Digest, DomainId, List},
    wire::{
        RpcRequestHarnessStartTurnBody as TurnSeed, StepBinding, ToolResult, TrustedContext,
        TurnOutcome,
    },
};
use avencrew_harness::{
    engine::{
        self, Candidate, Control, EngineConfig, EngineError, Generation, ModelRecord, ModelStep,
        Observation, Restoration, TurnState,
    },
    model::fixture::{
        normalize_fixture, FixtureBinding, FixtureError, FixtureFailure, FixtureInput,
        FixtureResult, FixtureUsage,
    },
};
use avencrew_store_sqlite::{LocalDispatch, LocalExecutionAccess, LocalStore, TurnError};
pub use launch::{prepare_fixture_turn, FixtureLaunch, PreparedFixtureTurn};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2_auth::{Digest as _, Sha256};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureSource {
    pub name: String,
    pub workspace_id: DomainId,
    pub audience_ids: Vec<DomainId>,
    pub version_id: DomainId,
    pub content: Value,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureEpisode {
    pub delay_ms: u32,
    pub input: FixtureInput,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureProfile {
    pub schema_version: String,
    pub provider: String,
    pub rate_micro: String,
    pub sources: Vec<FixtureSource>,
    pub episodes: Vec<FixtureEpisode>,
    #[serde(default)]
    pub output_format: avencrew_contracts::runtime::ArtifactFormat,
    #[serde(default)]
    pub patch_bases: std::collections::BTreeMap<String, String>,
}
fn hash(bytes: &[u8]) -> Digest {
    Digest::new(
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>(),
    )
    .expect("SHA-256 encoding")
}
fn id() -> DomainId {
    DomainId::new(uuid::Uuid::now_v7().to_string()).expect("OS UUIDv7 allocation")
}
fn fixed(n: u32) -> DomainId {
    DomainId::new(format!("019a5500-0000-7000-8000-{n:012x}"))
        .expect("fixed fixture capability identifier")
}
fn storage(error: TurnError) -> EngineError {
    match error {
        TurnError::StaleAuthority | TurnError::Cancelled | TurnError::NotFoundOrDenied => {
            EngineError::StaleAuthority
        }
        _ => EngineError::Storage,
    }
}
fn broker_storage(error: TurnError) -> BrokerError {
    match error {
        TurnError::Cancelled => BrokerError::Cancelled,
        TurnError::StaleAuthority => BrokerError::StaleAuthority,
        TurnError::NotFoundOrDenied => BrokerError::Denied,
        TurnError::IdempotencyMismatch => BrokerError::IdempotencyMismatch,
        TurnError::Unsupported => BrokerError::Unsupported,
        _ => BrokerError::Storage,
    }
}
fn access<'a>(
    sample: &'a clock::Sample,
    workspace: &'a DomainId,
    actor: &'a DomainId,
    turn: &'a DomainId,
) -> LocalExecutionAccess<'a> {
    LocalExecutionAccess::new(
        workspace,
        actor,
        turn,
        &sample.now,
        &sample.boot,
        &sample.monotonic,
    )
}

struct FixtureHost {
    store: Arc<Mutex<LocalStore>>,
    actor: DomainId,
    seed: TurnSeed,
    profile: FixtureProfile,
    compatibility: Digest,
    active: Option<LocalStep>,
    active_ordinal: u32,
}
impl FixtureHost {
    fn source(&self, p: &PreparedInvocation) -> Option<&FixtureSource> {
        let name = p.arguments().as_map().get("source_name")?.as_str()?;
        self.profile.sources.iter().find(|s| {
            s.name == name
                && s.workspace_id == p.context().workspace_id
                && s.audience_ids.contains(&p.context().principal_id)
        })
    }
    fn catalog(&self) -> Result<SealedCatalog, EngineError> {
        SealedCatalog::seal(vec![Binding{binding_id:fixed(18),name:"fixture.read".into(),capability_version_id:fixed(1018),adapter_digest:hash(b"avencrew.fixture-read/1"),effect:Effect::Read,
            input_schema:json!({"type":"object","additionalProperties":false,"properties":{"source_name":{"type":"string","minLength":1,"maxLength":64}},"required":["source_name"]}),
            output_schema:json!({"type":"object","additionalProperties":false,"properties":{"source_name":{"type":"string"},"source_version_id":{"type":"string"},"content":true},"required":["source_name","source_version_id","content"]})}]).map_err(|_|EngineError::InvalidState)
    }
    fn engine_step(step: &LocalStep) -> Result<ModelStep, EngineError> {
        let catalog =
            SealedCatalog::seal(step.bindings.clone()).map_err(|_| EngineError::InvalidState)?;
        Ok(ModelStep {
            broker: Broker::new(
                step.binding.clone(),
                step.context.clone(),
                step.lease.clone(),
                catalog,
            )
            .map_err(|_| EngineError::StaleAuthority)?,
            model_call_id: step.model_call_id.clone(),
            request_digest: step.request_digest.clone(),
            provider_profile_id: step.provider_profile_id.clone(),
            compatibility_digest: step.compatibility_digest.clone(),
            context_bytes: avencrew_contracts::canonical_bytes(
                &serde_json::to_vec(&step.request).map_err(|_| EngineError::InvalidState)?,
            )
            .map_err(|_| EngineError::InvalidState)?
            .len() as u32,
        })
    }
    fn current_step(&self, step: &ModelStep) -> Result<&LocalStep, EngineError> {
        self.active
            .as_ref()
            .filter(|s| {
                s.binding.step_id == step.broker.step().step_id
                    && s.model_call_id == step.model_call_id
            })
            .ok_or(EngineError::InvalidState)
    }
}

/// Consumes an already prepared turn and the same durable store as control transport.
/// The fixture program cannot provide authority, clocks, SQL, network or executable tools.
pub async fn execute_fixture_turn(
    store: Arc<Mutex<LocalStore>>,
    workspace: DomainId,
    actor: DomainId,
    turn: DomainId,
    profile: FixtureProfile,
    config: EngineConfig,
) -> Result<TurnOutcome, EngineError> {
    if profile.schema_version != "avencrew.fixture-execution/1"
        || profile.provider != "fixture"
        || profile.rate_micro != "0"
        || profile.episodes.is_empty()
        || profile.episodes.len() > 50
        || profile.sources.len() > 100
        || profile.episodes.iter().any(|e| e.delay_ms > 60_000)
        || profile
            .sources
            .iter()
            .any(|s| s.name.is_empty() || s.name.len() > 64 || s.audience_ids.len() > 100)
    {
        return Err(EngineError::InvalidState);
    }
    let mut names = std::collections::BTreeSet::new();
    if !profile.sources.iter().all(|s| names.insert(&s.name)) {
        return Err(EngineError::InvalidState);
    }
    let compatibility = hash(&record_bytes(&profile).map_err(|_| EngineError::InvalidState)?);
    let seed = store
        .lock()
        .await
        .read_local_turn(&workspace, &actor, &turn)
        .await
        .map_err(storage)?
        .seed;
    let mut host = FixtureHost {
        store,
        actor,
        seed: seed.clone(),
        profile,
        compatibility,
        active: None,
        active_ordinal: 0,
    };
    // Own the composed journal/harness future on the heap. Its nested static
    // ports must not consume a worker thread's stack merely by being composed.
    let future = engine::run_turn(&mut host, &seed, config);
    Box::pin(future).await
}

impl engine::Host for FixtureHost {
    async fn restore(&mut self, _: &TurnSeed) -> Result<Restoration, EngineError> {
        let sample = clock::sample().map_err(|_| EngineError::StaleAuthority)?;
        let mut store = self.store.lock().await;
        let future = store.local_execution_snapshot(
            &self.seed.run_spec.workspace_id,
            &self.actor,
            &self.seed.turn_id,
        );
        let snap = Box::pin(future).await.map_err(storage)?;
        if let Some(checkpoint) = &snap.current_checkpoint {
            let restored = store
                .read_checkpoint(
                    &self.seed.run_spec.workspace_id,
                    &self.actor,
                    &self.seed.run_spec.run_id,
                    checkpoint,
                )
                .await
                .map_err(|_| EngineError::Storage)?;
            if restored.coverage.context != "exact_fixture_ledger" {
                return Err(EngineError::InvalidState);
            }
        }
        if let Some(outcome) = snap.progress.outcome {
            return Ok(Restoration {
                state: snap.progress.state,
                pending_step: None,
                outcome: Some(outcome),
            });
        }
        let control = Box::pin(store.apply_local_controls(
            access(
                &sample,
                &self.seed.run_spec.workspace_id,
                &self.actor,
                &self.seed.turn_id,
            ),
            &id(),
        ))
        .await
        .map_err(storage)?;
        if !control.stopped
            && (!control.paused
                && (self.seed.lease.boot_id != sample.boot
                    || self.seed.lease.expires_at <= sample.now
                    || self.seed.lease.monotonic_deadline_ns.value() <= sample.monotonic.value()))
        {
            return Err(EngineError::StaleAuthority);
        }
        let snap = store
            .local_execution_snapshot(
                &self.seed.run_spec.workspace_id,
                &self.actor,
                &self.seed.turn_id,
            )
            .await
            .map_err(storage)?;
        if let Some(step) = &snap.step {
            if step.compatibility_digest != self.compatibility {
                return Err(EngineError::StaleAuthority);
            }
        }
        // Every prior seal must use this same immutable profile, even after an observed step.
        let context = store
            .local_execution_context(
                &self.seed.run_spec.workspace_id,
                &self.actor,
                &self.seed.turn_id,
            )
            .await
            .map_err(storage)?;
        for entry in context["history"]
            .as_array()
            .ok_or(EngineError::InvalidState)?
        {
            let step_id: DomainId = serde_json::from_value(entry["step_id"].clone())
                .map_err(|_| EngineError::InvalidState)?;
            let old = store
                .read_local_step(
                    &self.seed.run_spec.workspace_id,
                    &self.actor,
                    &self.seed.turn_id,
                    &step_id,
                )
                .await
                .map_err(storage)?;
            if old.compatibility_digest != self.compatibility {
                return Err(EngineError::StaleAuthority);
            }
        }
        self.active_ordinal = snap.progress.state.steps_started;
        self.active = snap.step;
        let pending_step = self.active.as_ref().map(Self::engine_step).transpose()?;
        Ok(Restoration {
            state: snap.progress.state,
            pending_step,
            outcome: None,
        })
    }
    async fn controls(&mut self, _: &TurnSeed, _: &TurnState) -> Result<Control, EngineError> {
        let sample = clock::sample().map_err(|_| EngineError::StaleAuthority)?;
        let mut store = self.store.lock().await;
        let control = Box::pin(store.apply_local_controls(
            access(
                &sample,
                &self.seed.run_spec.workspace_id,
                &self.actor,
                &self.seed.turn_id,
            ),
            &id(),
        ))
        .await
        .map_err(storage)?;
        if control.stopped {
            let context = store
                .local_execution_context(
                    &self.seed.run_spec.workspace_id,
                    &self.actor,
                    &self.seed.turn_id,
                )
                .await
                .map_err(storage)?;
            let mut unresolved = Vec::new();
            for step in context["history"]
                .as_array()
                .ok_or(EngineError::InvalidState)?
            {
                for p in step["invocations"]
                    .as_array()
                    .ok_or(EngineError::InvalidState)?
                {
                    if matches!(p["state"].as_str(), Some("dispatched" | "unknown")) {
                        unresolved.push(
                            serde_json::from_value(p["id"].clone())
                                .map_err(|_| EngineError::InvalidState)?,
                        );
                    }
                }
            }
            return Ok(Control::Stop {
                applied_input_cut: control.applied_input_cut,
                cleanup_refs: List::new(vec![]).map_err(|_| EngineError::InvalidState)?,
                unresolved_receipt_refs: List::new(unresolved)
                    .map_err(|_| EngineError::InvalidState)?,
            });
        }
        if control.paused {
            return Ok(Control::Pause {
                applied_input_cut: control.applied_input_cut,
            });
        }
        let budget = store
            .local_execution_budget(
                &self.seed.run_spec.workspace_id,
                &self.actor,
                &self.seed.turn_id,
            )
            .await
            .map_err(storage)?;
        if budget.overdrawn || (self.active.is_none() && !budget.can_start_model) {
            return Ok(Control::ResourceLimit {
                applied_input_cut: control.applied_input_cut,
            });
        }
        Ok(Control::Continue {
            applied_input_cut: control.applied_input_cut,
        })
    }
    async fn seal_step(
        &mut self,
        _: &TurnSeed,
        state: &TurnState,
    ) -> Result<ModelStep, EngineError> {
        let sample = clock::sample().map_err(|_| EngineError::StaleAuthority)?;
        let catalog = self.catalog()?;
        let context = self
            .store
            .lock()
            .await
            .local_execution_context(
                &self.seed.run_spec.workspace_id,
                &self.actor,
                &self.seed.turn_id,
            )
            .await
            .map_err(storage)?;
        let request = json!({"schema_version":"avencrew.fixture-context/1","objective_version_id":self.seed.run_spec.objective_version_id,"exact_ledger":context,"fixture_profile_digest":self.compatibility});
        let bytes = avencrew_contracts::canonical_bytes(
            &serde_json::to_vec(&request).map_err(|_| EngineError::InvalidState)?,
        )
        .map_err(|_| EngineError::InvalidState)?;
        let step_id = id();
        let binding:StepBinding=serde_json::from_value(json!({"step_id":step_id,"model_profile_id":self.seed.run_spec.model_profile_id,"model_adapter_digest":hash(b"avencrew.fixture-model/1"),"prompt_schema_version":"1.0","catalog_digest":catalog.digest(),"tool_binding_ids":catalog.bindings().iter().map(|b|&b.binding_id).collect::<Vec<_>>(),"instruction_digests":self.seed.run_spec.instruction_digests,"context_manifest_id":id(),"environment_capability_digest":hash(b"fixture-read-only/no-process/1"),"permission_generation":"1","deletion_generation":"0","sealed_at":sample.now})).map_err(|_|EngineError::InvalidState)?;
        let trusted:TrustedContext=serde_json::from_value(json!({"workspace_id":self.seed.run_spec.workspace_id,"project_id":null,"run_id":self.seed.run_spec.run_id,"attempt_id":self.seed.lease.attempt_id,"step_id":step_id,"principal_id":self.seed.run_spec.acting_principal_id,"epoch":self.seed.lease.epoch,"environment_id":null,"generation":null,"binding_id":fixed(18),"catalog_digest":catalog.digest(),"budget_hold_refs":[],"approval_ref":null})).map_err(|_|EngineError::InvalidState)?;
        let step = LocalStep {
            accepted_input_cut: state.applied_input_cut.clone(),
            candidate: Some(avencrew_contracts::runtime::CandidateAllocation {
                artifact_id: id(),
                version_id: id(),
                content_blob_id: id(),
                checks_manifest_id: id(),
                checks_blob_id: id(),
                check_result_id: id(),
            }),
            binding,
            context: trusted,
            lease: self.seed.lease.clone(),
            bindings: catalog.bindings().to_vec(),
            model_call_id: id(),
            request_digest: hash(&bytes),
            provider_profile_id: self.seed.run_spec.model_profile_id.clone(),
            compatibility_digest: self.compatibility.clone(),
            request,
        };
        let result = Self::engine_step(&step)?;
        self.active = Some(step);
        self.active_ordinal = state.steps_started + 1;
        Ok(result)
    }
    async fn model_record(
        &mut self,
        _: &TurnSeed,
        step: &ModelStep,
        state: &TurnState,
    ) -> Result<ModelRecord, EngineError> {
        let sealed = self.current_step(step)?.clone();
        let sample = clock::sample().map_err(|_| EngineError::StaleAuthority)?;
        let mut store = self.store.lock().await;
        if store
            .seal_local_step(
                access(
                    &sample,
                    &self.seed.run_spec.workspace_id,
                    &self.actor,
                    &self.seed.turn_id,
                ),
                &sealed,
                state,
                &id(),
            )
            .await
            .map_err(storage)?
        {
            return Ok(ModelRecord::Fresh);
        }
        let snap = store
            .local_execution_snapshot(
                &self.seed.run_spec.workspace_id,
                &self.actor,
                &self.seed.turn_id,
            )
            .await
            .map_err(storage)?;
        Ok(match snap.model {
            Some(result) => ModelRecord::Response(Box::new(result)),
            None => ModelRecord::Unresolved,
        })
    }
    async fn generate(&mut self, generation: &Generation<'_>) -> ModelCompletion {
        let Some(episode) = self
            .profile
            .episodes
            .get(self.active_ordinal.saturating_sub(1) as usize)
        else {
            return Err(FixtureFailure {
                code: FixtureError::InvalidFinish,
                usage: FixtureUsage::Unknown,
            });
        };
        let mut input = episode.input.clone();
        let step = generation.step();
        input.seed.model_call_id = step.model_call_id.clone();
        input.seed.step_id = step.broker.step().step_id.clone();
        input.seed.provider_profile_id = step.provider_profile_id.clone();
        input.seed.compatibility_digest = step.compatibility_digest.clone();
        input.seed.bindings = vec![FixtureBinding {
            name: "fixture.read".into(),
            binding_id: fixed(18),
        }];
        input.seed.invocation_slots = input.seed.invocation_slots.iter().map(|_| id()).collect();
        input.seed.usage_ref = id();
        tokio::task::yield_now().await;
        let deadline = std::time::Instant::now()
            + std::time::Duration::from_millis(u64::from(episode.delay_ms));
        while std::time::Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            tokio::time::sleep(remaining.min(std::time::Duration::from_millis(50))).await;
            match self
                .store
                .lock()
                .await
                .local_execution_stop_requested(
                    &self.seed.run_spec.workspace_id,
                    &self.actor,
                    &self.seed.turn_id,
                )
                .await
            {
                Ok(false) => {}
                _ => {
                    return Err(FixtureFailure {
                        code: FixtureError::Cancelled,
                        usage: FixtureUsage::Unknown,
                    })
                }
            }
        }
        normalize_fixture(&input)
    }
    async fn record_model(
        &mut self,
        _: &TurnSeed,
        step: &ModelStep,
        result: &ModelCompletion,
        state: &TurnState,
    ) -> Result<(), EngineError> {
        let sealed = self.current_step(step)?.clone();
        let sample = clock::sample().map_err(|_| EngineError::StaleAuthority)?;
        self.store
            .lock()
            .await
            .record_local_model(
                access(
                    &sample,
                    &self.seed.run_spec.workspace_id,
                    &self.actor,
                    &self.seed.turn_id,
                ),
                &sealed,
                result,
                state,
                &id(),
                &id(),
            )
            .await
            .map_err(storage)
    }
    async fn observe(
        &mut self,
        _: &TurnSeed,
        step: &ModelStep,
        _: &[Observation],
        state: &TurnState,
    ) -> Result<Digest, EngineError> {
        let sample = clock::sample().map_err(|_| EngineError::StaleAuthority)?;
        let result = self
            .store
            .lock()
            .await
            .observe_local_step(
                access(
                    &sample,
                    &self.seed.run_spec.workspace_id,
                    &self.actor,
                    &self.seed.turn_id,
                ),
                &step.broker.step().step_id,
                state,
                &id(),
            )
            .await
            .map_err(storage);
        if result.is_ok() {
            self.active = None;
        }
        result
    }
    async fn stage_candidate(
        &mut self,
        _: &TurnSeed,
        step: &ModelStep,
        response: &FixtureResult,
    ) -> Result<Candidate, EngineError> {
        let text = response
            .response
            .items
            .as_slice()
            .iter()
            .filter_map(|item| match item {
                avencrew_contracts::wire::NormalizedModelResponseItemsItem::Text { text } => {
                    Some(text.as_str())
                }
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        let validation = avencrew_harness::artifacts::validate(
            self.profile.output_format,
            text.as_bytes(),
            &self.profile.patch_bases,
        );
        let allocation = self
            .current_step(step)?
            .candidate
            .clone()
            .ok_or(EngineError::InvalidState)?;
        let sample = clock::sample().map_err(|_| EngineError::StaleAuthority)?;
        let receipt = self
            .store
            .lock()
            .await
            .publish_local_artifact(avencrew_store_sqlite::LocalArtifactPublication {
                access: access(
                    &sample,
                    &self.seed.run_spec.workspace_id,
                    &self.actor,
                    &self.seed.turn_id,
                ),
                target: avencrew_store_sqlite::LocalArtifactTarget::Candidate {
                    step: &step.broker.step().step_id,
                    artifact: &allocation.artifact_id,
                },
                version: &allocation.version_id,
                content_blob: &allocation.content_blob_id,
                checks_manifest: &allocation.checks_manifest_id,
                checks_blob: &allocation.checks_blob_id,
                result: &allocation.check_result_id,
                bytes: text.as_bytes(),
                validation: &validation,
            })
            .await
            .map_err(storage)?;
        Ok(Candidate {
            artifacts: List::new(vec![receipt.artifact]).map_err(|_| EngineError::InvalidState)?,
            checks: List::new(vec![receipt.check_result_id])
                .map_err(|_| EngineError::InvalidState)?,
        })
    }
    async fn checkpoint(
        &mut self,
        _: &TurnSeed,
        state: &TurnState,
    ) -> Result<DomainId, EngineError> {
        let sample = clock::sample().map_err(|_| EngineError::StaleAuthority)?;
        self.store
            .lock()
            .await
            .checkpoint_local_execution(avencrew_store_sqlite::LocalExecutionCheckpoint {
                access: access(
                    &sample,
                    &self.seed.run_spec.workspace_id,
                    &self.actor,
                    &self.seed.turn_id,
                ),
                state,
                checkpoint: &id(),
                manifest: &id(),
                coverage_entry: &id(),
                execution_entry: &id(),
                bundle_blob: &id(),
                event: &id(),
                event_blob: &id(),
            })
            .await
            .map_err(storage)
    }
    async fn commit_outcome(
        &mut self,
        _: &TurnSeed,
        state: &TurnState,
        outcome: &TurnOutcome,
    ) -> Result<(), EngineError> {
        let sample = clock::sample().map_err(|_| EngineError::StaleAuthority)?;
        self.store
            .lock()
            .await
            .commit_local_outcome(
                access(
                    &sample,
                    &self.seed.run_spec.workspace_id,
                    &self.actor,
                    &self.seed.turn_id,
                ),
                state,
                outcome,
                &id(),
            )
            .await
            .map_err(storage)
    }
}

impl broker::Host for FixtureHost {
    async fn commit_proposals(
        &mut self,
        step: &StepBinding,
        proposals: &[PreparedInvocation],
    ) -> Result<(), BrokerError> {
        if self.active.as_ref().map(|s| &s.binding) != Some(step) {
            return Err(BrokerError::UnsealedBinding);
        }
        let budget = self
            .store
            .lock()
            .await
            .local_execution_budget(
                &self.seed.run_spec.workspace_id,
                &self.actor,
                &self.seed.turn_id,
            )
            .await
            .map_err(broker_storage)?;
        if budget.tool_proposals.value() + proposals.len() as u64 > budget.tool_limit.value() {
            return Err(BrokerError::LimitExceeded);
        }
        let records = proposals
            .iter()
            .map(PreparedInvocation::journal_record)
            .collect::<Result<Vec<_>, _>>()?;
        let sample = clock::sample().map_err(|_| BrokerError::StaleAuthority)?;
        self.store
            .lock()
            .await
            .commit_local_proposals(
                access(
                    &sample,
                    &self.seed.run_spec.workspace_id,
                    &self.actor,
                    &self.seed.turn_id,
                ),
                &records,
                &id(),
            )
            .await
            .map_err(broker_storage)
    }
    async fn committed_digest(
        &mut self,
        p: &PreparedInvocation,
    ) -> Result<Option<Digest>, BrokerError> {
        let saved = self
            .store
            .lock()
            .await
            .read_local_proposal(
                &self.seed.run_spec.workspace_id,
                &self.actor,
                p.invocation_id(),
            )
            .await
            .map_err(broker_storage)?;
        Ok(Some(saved.digest))
    }
    async fn current_policy(
        &mut self,
        p: &PreparedInvocation,
    ) -> Result<PolicySnapshot, BrokerError> {
        let sample = clock::sample().map_err(|_| BrokerError::StaleAuthority)?;
        self.store
            .lock()
            .await
            .authorize_local_execution(
                &access(
                    &sample,
                    &self.seed.run_spec.workspace_id,
                    &self.actor,
                    &self.seed.turn_id,
                ),
                false,
            )
            .await
            .map_err(broker_storage)?;
        let snap = self
            .store
            .lock()
            .await
            .local_execution_snapshot(
                &self.seed.run_spec.workspace_id,
                &self.actor,
                &self.seed.turn_id,
            )
            .await
            .map_err(broker_storage)?;
        if snap.step.as_ref().is_none_or(|step| {
            step.binding.step_id != p.context().step_id
                || step.accepted_input_cut != snap.progress.state.applied_input_cut
        }) {
            return Err(BrokerError::StaleAuthority);
        }
        let budget = self
            .store
            .lock()
            .await
            .local_execution_budget(
                &self.seed.run_spec.workspace_id,
                &self.actor,
                &self.seed.turn_id,
            )
            .await
            .map_err(broker_storage)?;
        Ok(PolicySnapshot {
            context: p.context().clone(),
            lease: self.seed.lease.clone(),
            now: sample.now,
            boot_id: sample.boot,
            monotonic_now_ns: sample.monotonic,
            permission_generation: Counter::new("1").map_err(|_| BrokerError::StaleAuthority)?,
            deletion_generation: Counter::new("0").map_err(|_| BrokerError::StaleAuthority)?,
            principal_active: true,
            workspace_allowed: true,
            project_allowed: true,
            task_allowed: true,
            worker_allowed: true,
            installation_allowed: true,
            environment_allowed: true,
            target_allowed: self.source(p).is_some(),
            budget_reserved: budget.can_dispatch,
            dispatch_supported: true,
            cancellation_requested: false,
            unresolved_effect: false,
            parent_active_same_step: false,
            model_call_id: self.active.as_ref().map(|s| s.model_call_id.clone()),
            max_result_bytes: self.seed.limits.output_bytes.value() as u32,
        })
    }
    async fn claim_dispatch(
        &mut self,
        p: &PreparedInvocation,
        admission: &DispatchAdmission,
    ) -> Result<Dispatch, BrokerError> {
        let policy = self.current_policy(p).await?;
        if policy.digest()? != *admission.policy_digest()
            || p.digest()? != *admission.invocation_digest()
        {
            return Err(BrokerError::StaleAuthority);
        }
        let sample = clock::sample().map_err(|_| BrokerError::StaleAuthority)?;
        let claimed = self
            .store
            .lock()
            .await
            .claim_local_dispatch(
                access(
                    &sample,
                    &self.seed.run_spec.workspace_id,
                    &self.actor,
                    &self.seed.turn_id,
                ),
                &p.journal_record()?,
                admission.policy_digest(),
                &id(),
            )
            .await
            .map_err(broker_storage)?;
        Ok(match claimed {
            LocalDispatch::Fresh => Dispatch::Started,
            LocalDispatch::Result(r) => Dispatch::Replay(r),
            LocalDispatch::Unknown => Dispatch::Unknown,
        })
    }
    async fn execute(&mut self, execution: &Execution<'_>) -> Result<ToolResult, BrokerError> {
        tokio::task::yield_now().await;
        let p = execution.invocation();
        self.current_policy(p).await?;
        let source = self.source(p).ok_or(BrokerError::Denied)?;
        serde_json::from_value(json!({"status":"succeeded","invocation_id":p.invocation_id(),"schema_version":"1.0","artifact_refs":[],"receipt_ref":null,"truncated":false,"next_cursor":null,"usage_refs":[],"data":{"binding_id":p.context().binding_id,"value":{"source_name":source.name,"source_version_id":source.version_id,"content":source.content}}})).map_err(|_|BrokerError::InvalidResult)
    }
    async fn record_result(
        &mut self,
        execution: &Execution<'_>,
        result: &ToolResult,
    ) -> Result<(), BrokerError> {
        let sample = clock::sample().map_err(|_| BrokerError::StaleAuthority)?;
        self.store
            .lock()
            .await
            .record_local_tool_result(
                access(
                    &sample,
                    &self.seed.run_spec.workspace_id,
                    &self.actor,
                    &self.seed.turn_id,
                ),
                &execution.invocation().journal_record()?,
                Some(result),
                &id(),
            )
            .await
            .map_err(broker_storage)
    }
    async fn record_unknown(&mut self, execution: &Execution<'_>) -> Result<(), BrokerError> {
        let sample = clock::sample().map_err(|_| BrokerError::StaleAuthority)?;
        self.store
            .lock()
            .await
            .record_local_tool_result(
                access(
                    &sample,
                    &self.seed.run_spec.workspace_id,
                    &self.actor,
                    &self.seed.turn_id,
                ),
                &execution.invocation().journal_record()?,
                None,
                &id(),
            )
            .await
            .map_err(broker_storage)
    }
}

#[cfg(test)]
mod tests;
