//! One bounded turn loop. The trusted host owns authority, durable commits,
//! checkpoint capture, model IO and candidate staging. No database or scheduler
//! is reachable here; final model text cannot produce business completion.
use crate::model::fixture::{
    commit_fixture_proposals, FixtureFailure, FixtureResult, FixtureUsage,
};
use avencrew_broker::{Broker, BrokerError, Invocation, Outcome as InvocationOutcome};
use avencrew_contracts::{
    encode_message,
    scalars::{Counter, Digest, DomainId, List, Nullable, Text},
    wire::{
        ArtifactRef, ErrorCode, NormalizedModelResponseFinish as Finish,
        NormalizedModelResponseItemsItem as Item, ProtocolVersion,
        RpcRequestHarnessStartTurnBody as TurnSeed, SafeError, ToolResult, TurnOutcome,
        TurnOutcomeFailedRetryClass as Retry, TurnOutcomeYieldedReason as Yield, WireMessage,
    },
    Validate,
};
use std::{
    io::Write,
    time::{Duration, Instant},
};

/// Payload-free failures. A commit failure never acknowledges saved progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineError {
    InvalidState,
    StaleAuthority,
    Storage,
    UnknownOutcome,
    InvalidResponse,
}
impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for EngineError {}

/// Exact engine counters retained by the host together with each transition.
pub use avencrew_contracts::runtime::TurnState;

/// A host-sealed request. Request digest covers the bounded context and settings;
/// the host resolves its bytes from the pinned manifest, not mutable live state.
pub struct ModelStep {
    pub broker: Broker,
    pub model_call_id: DomainId,
    pub request_digest: Digest,
    pub provider_profile_id: DomainId,
    pub compatibility_digest: Digest,
    pub context_bytes: u32,
}
pub type ModelCompletion = Result<FixtureResult, FixtureFailure>;
pub enum ModelRecord {
    /// Returned only by the atomic first admission of a new request intent.
    Fresh,
    Response(Box<ModelCompletion>),
    /// An intent without an authoritative response must not call inference again.
    Unresolved,
}
pub struct Restoration {
    pub state: TurnState,
    pub pending_step: Option<ModelStep>,
    pub outcome: Option<TurnOutcome>,
}
#[derive(Clone)]
pub enum Control {
    Continue {
        applied_input_cut: Counter,
    },
    Pause {
        applied_input_cut: Counter,
    },
    ResourceLimit {
        applied_input_cut: Counter,
    },
    Stop {
        applied_input_cut: Counter,
        cleanup_refs: List<DomainId, 0, 100>,
        unresolved_receipt_refs: List<DomainId, 0, 100>,
    },
}
/// Ordered committed observations; transport/authority failures stay explicit.
pub struct Observation {
    pub invocation_id: DomainId,
    pub result: Result<InvocationOutcome, BrokerError>,
}
pub struct Candidate {
    pub artifacts: List<ArtifactRef, 0, 100>,
    pub checks: List<DomainId, 0, 100>,
}
/// Cannot be constructed or retained by harness callers. Inference requires an
/// acknowledged intent and consumes the same immutable step the engine holds.
pub struct Generation<'a> {
    step: &'a ModelStep,
}
impl Generation<'_> {
    pub fn step(&self) -> &ModelStep {
        self.step
    }
}

/// Trusted coordinator port. Each acknowledgement denotes a durable atomic
/// transition under current run/attempt/epoch authority, never an in-memory ack.
/// restore validates checkpoint/journal compatibility and reconciles unknowns;
/// controls commits ordered dispositions with stop priority. model_record seals
/// step+intent+state once (same ID/different digest conflicts). record_model
/// commits response AND known/unknown usage before any proposal is accepted.
/// Broker proposal commits must require that retained response. observe commits
/// provider-order observations and derived progress/no-progress state atomically
/// (increment iff progress digest is unchanged, otherwise reset to zero).
/// checkpoint captures exact resumable state;
/// commit_outcome publishes checkpoint pointer and outcome atomically. No model
/// or adapter IO occurs inside transactions. The host must atomically fence
/// model/tool admissions against stop, authority, resource and budget changes.
#[allow(async_fn_in_trait)]
pub trait Host: avencrew_broker::Host {
    async fn restore(&mut self, seed: &TurnSeed) -> Result<Restoration, EngineError>;
    async fn controls(
        &mut self,
        seed: &TurnSeed,
        state: &TurnState,
    ) -> Result<Control, EngineError>;
    async fn seal_step(
        &mut self,
        seed: &TurnSeed,
        state: &TurnState,
    ) -> Result<ModelStep, EngineError>;
    async fn model_record(
        &mut self,
        seed: &TurnSeed,
        step: &ModelStep,
        state: &TurnState,
    ) -> Result<ModelRecord, EngineError>;
    async fn generate(&mut self, generation: &Generation<'_>) -> ModelCompletion;
    async fn record_model(
        &mut self,
        seed: &TurnSeed,
        step: &ModelStep,
        result: &ModelCompletion,
        state: &TurnState,
    ) -> Result<(), EngineError>;
    async fn observe(
        &mut self,
        seed: &TurnSeed,
        step: &ModelStep,
        observations: &[Observation],
        state: &TurnState,
    ) -> Result<Digest, EngineError>;
    async fn stage_candidate(
        &mut self,
        seed: &TurnSeed,
        step: &ModelStep,
        response: &FixtureResult,
    ) -> Result<Candidate, EngineError>;
    async fn checkpoint(
        &mut self,
        seed: &TurnSeed,
        state: &TurnState,
    ) -> Result<DomainId, EngineError>;
    async fn commit_outcome(
        &mut self,
        seed: &TurnSeed,
        state: &TurnState,
        outcome: &TurnOutcome,
    ) -> Result<(), EngineError>;
}

/// Local engine settings only lower bounded continuation thresholds. Model,
/// time and output caps are the already admitted wire Limits.
#[derive(Debug, Clone, Copy)]
pub struct EngineConfig {
    pub context_bytes: u32,
    pub no_progress_steps: u32,
}
impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            context_bytes: 1_048_576,
            no_progress_steps: 3,
        }
    }
}
enum End {
    Yield(Yield),
    Pause,
    Stop(List<DomainId, 0, 100>, List<DomainId, 0, 100>),
    Candidate(Candidate),
    Fail(ErrorCode, Retry),
}
async fn boundary<H: Host>(
    host: &mut H,
    seed: &TurnSeed,
    state: &mut TurnState,
    pinned: bool,
) -> Result<Option<End>, EngineError> {
    Ok(match host.controls(seed, state).await? {
        Control::Stop {
            applied_input_cut,
            cleanup_refs,
            unresolved_receipt_refs,
        } => {
            if applied_input_cut.value() < state.applied_input_cut.value() {
                return Err(EngineError::InvalidState);
            }
            state.applied_input_cut = applied_input_cut;
            Some(End::Stop(cleanup_refs, unresolved_receipt_refs))
        }
        Control::Pause { applied_input_cut } => {
            if applied_input_cut.value() < state.applied_input_cut.value() {
                return Err(EngineError::InvalidState);
            }
            state.applied_input_cut = applied_input_cut;
            Some(End::Pause)
        }
        Control::ResourceLimit { applied_input_cut } => {
            if applied_input_cut.value() < state.applied_input_cut.value() {
                return Err(EngineError::InvalidState);
            }
            state.applied_input_cut = applied_input_cut;
            Some(End::Fail(ErrorCode::ResourceLimit, Retry::Remediation))
        }
        Control::Continue { applied_input_cut } => {
            if applied_input_cut.value() < state.applied_input_cut.value() {
                return Err(EngineError::InvalidState);
            }
            let changed = applied_input_cut != state.applied_input_cut;
            state.applied_input_cut = applied_input_cut;
            if pinned && changed {
                Some(End::Yield(Yield::User))
            } else {
                None
            }
        }
    })
}
async fn finish<H: Host>(
    host: &mut H,
    seed: &TurnSeed,
    state: &TurnState,
    end: End,
) -> Result<TurnOutcome, EngineError> {
    let mut current = state.clone();
    let end = if matches!(end, End::Stop(..)) {
        end
    } else {
        match boundary(host, seed, &mut current, true).await? {
            Some(End::Stop(cleanup, unresolved)) => End::Stop(cleanup, unresolved),
            Some(End::Pause) => End::Pause,
            Some(control) if matches!(end, End::Candidate(_) | End::Yield(_)) => control,
            _ => end,
        }
    };
    let state = &current;
    let checkpoint = host.checkpoint(seed, state).await?;
    checkpoint
        .validate()
        .map_err(|_| EngineError::InvalidState)?;
    let outcome = match end {
        End::Yield(reason) => TurnOutcome::Yielded {
            reason,
            checkpoint_id: checkpoint,
        },
        End::Pause => TurnOutcome::Paused {
            reason: Yield::User,
            checkpoint_id: checkpoint,
        },
        End::Stop(cleanup_refs, unresolved_receipt_refs) => TurnOutcome::Cancelled {
            checkpoint_id: Nullable(Some(checkpoint)),
            cleanup_refs,
            unresolved_receipt_refs,
        },
        End::Candidate(candidate) => TurnOutcome::CandidateResult {
            objective_version_id: seed.run_spec.objective_version_id.clone(),
            artifact_refs: candidate.artifacts,
            check_result_refs: candidate.checks,
            checkpoint_id: checkpoint,
        },
        End::Fail(code, retry_class) => TurnOutcome::Failed {
            error: SafeError {
                code,
                message: Text::new("turn stopped at a retained boundary")
                    .map_err(|_| EngineError::InvalidState)?,
                evidence_refs: List::new(vec![]).map_err(|_| EngineError::InvalidState)?,
            },
            retry_class,
            checkpoint_id: Nullable(Some(checkpoint)),
        },
    };
    validate_outcome(seed, &outcome)?;
    host.commit_outcome(seed, state, &outcome).await?;
    Ok(outcome)
}
fn validate_outcome(seed: &TurnSeed, outcome: &TurnOutcome) -> Result<(), EngineError> {
    if let TurnOutcome::CandidateResult {
        objective_version_id,
        ..
    } = outcome
    {
        if *objective_version_id != seed.run_spec.objective_version_id {
            return Err(EngineError::StaleAuthority);
        }
    }
    encode_message(&WireMessage::TurnOutcome {
        schema_version: ProtocolVersion::V10,
        payload: Box::new(outcome.clone()),
    })
    .map_err(|_| EngineError::InvalidState)?;
    Ok(())
}
fn validate_step(seed: &TurnSeed, step: &ModelStep) -> Result<(), EngineError> {
    let context = step.broker.context();
    let lease = step.broker.lease();
    if context.run_id != seed.run_spec.run_id
        || context.attempt_id != seed.lease.attempt_id
        || context.workspace_id != seed.run_spec.workspace_id
        || context.principal_id != seed.run_spec.acting_principal_id
        || context.project_id != seed.run_spec.project_id
        || context.epoch != seed.lease.epoch
        || context.environment_id != seed.lease.environment_id
        || context.generation != seed.lease.generation
        || step.broker.step().model_profile_id != seed.run_spec.model_profile_id
        || lease.authority != seed.lease.authority
        || lease.placement != seed.lease.placement
        || lease.boot_id != seed.lease.boot_id
    {
        return Err(EngineError::StaleAuthority);
    }
    step.provider_profile_id
        .validate()
        .map_err(|_| EngineError::InvalidState)?;
    step.compatibility_digest
        .validate()
        .map_err(|_| EngineError::InvalidState)?;
    step.model_call_id
        .validate()
        .map_err(|_| EngineError::InvalidState)?;
    step.request_digest
        .validate()
        .map_err(|_| EngineError::InvalidState)?;
    Ok(())
}
// Direct Rust host values receive the same bound as decoded model streams,
// before cloning, traversing the wire validator or committing a response.
struct Count {
    used: usize,
    limit: usize,
}
impl Write for Count {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.used = self
            .used
            .checked_add(bytes.len())
            .filter(|n| *n <= self.limit)
            .ok_or_else(|| std::io::Error::other("response bound"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn valid_completion(step: &ModelStep, result: &ModelCompletion, output_bytes: u64) -> bool {
    let Ok(result) = result else { return true };
    if result.response.model_call_id != step.model_call_id
        || result.response.step_id != step.broker.step().step_id
    {
        return false;
    }
    for item in result.response.items.as_slice() {
        match item {
            Item::ToolProposal { .. } => {
                if result.response.finish != Finish::Tools
                    || Invocation::from_model(
                        step.model_call_id.clone(),
                        step.model_call_id.clone(),
                        item,
                    )
                    .is_err()
                {
                    return false;
                }
            }
            Item::OpaqueContinuation {
                provider_profile_id,
                compatibility_digest,
                ..
            } if *provider_profile_id != step.provider_profile_id
                || *compatibility_digest != step.compatibility_digest =>
            {
                return false
            }
            _ => {}
        }
    }
    if result.response.finish != Finish::Tools && !result.proposal_identities.is_empty() {
        return false;
    }
    serde_json::to_writer(
        Count {
            used: 0,
            limit: output_bytes as usize,
        },
        &result.response.items,
    )
    .is_ok()
        && serde_json::to_writer(
            Count {
                used: 0,
                limit: 1_048_576,
            },
            result,
        )
        .is_ok()
        && encode_message(&WireMessage::NormalizedModelResponse {
            schema_version: ProtocolVersion::V10,
            payload: Box::new(result.response.clone()),
        })
        .is_ok()
}
fn usage(result: &ModelCompletion) -> FixtureUsage {
    match result {
        Ok(result) => result.usage.clone(),
        Err(failure) => failure.usage.clone(),
    }
}
fn elapsed(start: Instant, restored_ms: u64) -> u64 {
    restored_ms.saturating_add(u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX))
}
fn remaining(seed: &TurnSeed, state: &TurnState) -> Duration {
    Duration::from_millis(
        seed.limits
            .active_wall_ms
            .value()
            .saturating_sub(state.active_elapsed_ms),
    )
}

async fn observe<H: Host>(
    host: &mut H,
    seed: &TurnSeed,
    step: &ModelStep,
    observations: &[Observation],
    state: &mut TurnState,
) -> Result<(), EngineError> {
    let progress = host.observe(seed, step, observations, state).await?;
    state.no_progress_steps = if progress == state.progress_digest {
        state.no_progress_steps + 1
    } else {
        0
    };
    state.progress_digest = progress;
    Ok(())
}

/// Run or resume one already coordinator-admitted turn. No implicit retries,
/// live task handles, waits or business completion are created by this profile.
/// IO/commit errors return without acknowledging a retained outcome.
pub async fn run_turn<H: Host>(
    host: &mut H,
    seed: &TurnSeed,
    config: EngineConfig,
) -> Result<TurnOutcome, EngineError> {
    seed.validate().map_err(|_| EngineError::InvalidState)?;
    if seed.lease.run_id != seed.run_spec.run_id
        || seed.lease.placement != seed.run_spec.placement
        || seed.accepted_input_cut.value() < seed.run_spec.accepted_input_seq.value()
        || !(1..=1_048_576).contains(&config.context_bytes)
        || !(1..=50).contains(&config.no_progress_steps)
    {
        return Err(EngineError::InvalidState);
    }
    let started = Instant::now();
    let restored = host.restore(seed).await?;
    let mut state = restored.state;
    if state.steps_started as u64 > seed.limits.model_steps.value()
        || state.no_progress_steps > 50
        || (restored.pending_step.is_some() && state.steps_started == 0)
    {
        return Err(EngineError::InvalidState);
    }
    state
        .applied_input_cut
        .validate()
        .map_err(|_| EngineError::InvalidState)?;
    state
        .progress_digest
        .validate()
        .map_err(|_| EngineError::InvalidState)?;
    if let Some(outcome) = restored.outcome {
        if restored.pending_step.is_some() {
            return Err(EngineError::InvalidState);
        }
        validate_outcome(seed, &outcome)?;
        return Ok(outcome); // projection replay performs no model, tool or checkpoint work
    }
    let restored_ms = state.active_elapsed_ms;
    let mut pending = restored.pending_step;
    loop {
        state.active_elapsed_ms = elapsed(started, restored_ms);
        if let Some(end) = boundary(host, seed, &mut state, false).await? {
            return finish(host, seed, &state, end).await;
        }
        if state.applied_input_cut.value() < seed.accepted_input_cut.value() {
            return Err(EngineError::InvalidState);
        }
        state.active_elapsed_ms = elapsed(started, restored_ms);
        state.active_elapsed_ms = elapsed(started, restored_ms);
        if remaining(seed, &state).is_zero() {
            return finish(host, seed, &state, End::Yield(Yield::TurnLimit)).await;
        }
        if state.no_progress_steps >= config.no_progress_steps {
            return finish(
                host,
                seed,
                &state,
                End::Fail(ErrorCode::ResourceLimit, Retry::Remediation),
            )
            .await;
        }
        let resuming = pending.is_some();
        if !resuming && state.steps_started as u64 >= seed.limits.model_steps.value() {
            return finish(host, seed, &state, End::Yield(Yield::TurnLimit)).await;
        }
        let step = match pending.take() {
            Some(step) => step,
            None => host.seal_step(seed, &state).await?,
        };
        validate_step(seed, &step)?;
        // Reserve declared output headroom as well as the supplied context.
        if u64::from(step.context_bytes) + seed.limits.output_bytes.value()
            > u64::from(config.context_bytes)
        {
            return finish(host, seed, &state, End::Yield(Yield::ContextLimit)).await;
        }
        if !resuming {
            state.steps_started += 1;
        }
        state.active_elapsed_ms = elapsed(started, restored_ms);
        if remaining(seed, &state).is_zero() {
            return finish(host, seed, &state, End::Yield(Yield::TurnLimit)).await;
        }
        let record = host.model_record(seed, &step, &state).await?;
        let result = match record {
            ModelRecord::Fresh if !resuming => {
                state.active_elapsed_ms = elapsed(started, restored_ms);
                let result = match tokio::time::timeout(
                    remaining(seed, &state),
                    host.generate(&Generation { step: &step }),
                )
                .await
                {
                    Ok(result) => result,
                    Err(_) => {
                        let result = Err(FixtureFailure {
                            code: crate::model::fixture::FixtureError::InvalidFinish,
                            usage: FixtureUsage::Unknown,
                        });
                        state.active_elapsed_ms = elapsed(started, restored_ms);
                        host.record_model(seed, &step, &result, &state).await?;
                        return finish(
                            host,
                            seed,
                            &state,
                            End::Fail(ErrorCode::Timeout, Retry::ReconcileFirst),
                        )
                        .await;
                    }
                };
                let result = if valid_completion(&step, &result, seed.limits.output_bytes.value()) {
                    result
                } else {
                    Err(FixtureFailure {
                        code: crate::model::fixture::FixtureError::InvalidResponse,
                        usage: usage(&result),
                    })
                };
                state.active_elapsed_ms = elapsed(started, restored_ms);
                host.record_model(seed, &step, &result, &state).await?;
                result
            }
            ModelRecord::Response(result) => {
                if !valid_completion(&step, &result, seed.limits.output_bytes.value()) {
                    return Err(EngineError::InvalidResponse);
                }
                *result
            }
            _ => {
                return finish(
                    host,
                    seed,
                    &state,
                    End::Fail(ErrorCode::NeedsReconciliation, Retry::ReconcileFirst),
                )
                .await
            }
        };
        let result = match result {
            Ok(result) => result,
            Err(_) => {
                return finish(
                    host,
                    seed,
                    &state,
                    End::Fail(ErrorCode::InvalidMessage, Retry::Remediation),
                )
                .await
            }
        };
        // Controls are drained again after inference and before each tool. The
        // host independently fences the race between this boundary and admission.
        state.active_elapsed_ms = elapsed(started, restored_ms);
        if let Some(end) = boundary(host, seed, &mut state, true).await? {
            return finish(host, seed, &state, end).await;
        }
        state.active_elapsed_ms = elapsed(started, restored_ms);
        if remaining(seed, &state).is_zero() {
            return finish(host, seed, &state, End::Yield(Yield::TurnLimit)).await;
        }
        match result.response.finish {
            Finish::Final => {
                let candidate = host.stage_candidate(seed, &step, &result).await?;
                return finish(host, seed, &state, End::Candidate(candidate)).await;
            }
            Finish::Length => {
                return finish(host, seed, &state, End::Yield(Yield::ContextLimit)).await
            }
            Finish::Refused => {
                return finish(
                    host,
                    seed,
                    &state,
                    End::Fail(ErrorCode::Denied, Retry::Remediation),
                )
                .await
            }
            Finish::Interrupted => {
                return finish(
                    host,
                    seed,
                    &state,
                    End::Fail(ErrorCode::UnknownOutcome, Retry::ReconcileFirst),
                )
                .await
            }
            Finish::Failed => {
                let code = result
                    .response
                    .diagnostic_error
                    .0
                    .as_ref()
                    .map_or(ErrorCode::Unavailable, |error| error.code.clone());
                let retry = if matches!(code, ErrorCode::RateLimited | ErrorCode::Unavailable) {
                    Retry::SafeRead
                } else {
                    Retry::Remediation
                };
                return finish(host, seed, &state, End::Fail(code, retry)).await;
            }
            Finish::Tools => {}
        }
        let proposals = match commit_fixture_proposals(
            &step.broker,
            host,
            &step.model_call_id,
            &result,
        )
        .await
        {
            Ok(proposals) => proposals,
            Err(BrokerError::Storage) => return Err(EngineError::Storage),
            Err(BrokerError::UnknownOutcome) => return Err(EngineError::UnknownOutcome),
            Err(BrokerError::LimitExceeded) => {
                return finish(
                    host,
                    seed,
                    &state,
                    End::Fail(ErrorCode::ResourceLimit, Retry::Remediation),
                )
                .await
            }

            Err(_) => {
                return finish(
                    host,
                    seed,
                    &state,
                    End::Fail(ErrorCode::InvalidArgument, Retry::Remediation),
                )
                .await
            }
        };
        let mut observations = Vec::with_capacity(proposals.len());
        for proposal in proposals {
            state.active_elapsed_ms = elapsed(started, restored_ms);
            if let Some(end) = boundary(host, seed, &mut state, true).await? {
                return finish(host, seed, &state, end).await;
            }
            state.active_elapsed_ms = elapsed(started, restored_ms);
            if remaining(seed, &state).is_zero() {
                return finish(host, seed, &state, End::Yield(Yield::TurnLimit)).await;
            }
            let request = Invocation {
                invocation_id: proposal.invocation_id().clone(),
                binding_id: proposal.context().binding_id.clone(),
                arguments: proposal.arguments().clone(),
                origin: proposal.origin().clone(),
            };
            let invoked =
                tokio::time::timeout(remaining(seed, &state), step.broker.invoke(host, request))
                    .await;
            let result = invoked.unwrap_or(Err(BrokerError::UnknownOutcome));
            let unresolved = matches!(
                &result,
                Ok(InvocationOutcome::Pending | InvocationOutcome::Unknown)
                    | Err(BrokerError::UnknownOutcome | BrokerError::Storage)
            );
            let unresolved = unresolved
                || matches!(&result,
                Ok(InvocationOutcome::Result(result)) if matches!(result.as_ref(), ToolResult::Unknown { .. } | ToolResult::Lost { .. }));
            let denied = matches!(
                &result,
                Err(BrokerError::StaleAuthority
                    | BrokerError::Denied
                    | BrokerError::Cancelled
                    | BrokerError::Unsupported)
            );
            observations.push(Observation {
                invocation_id: proposal.invocation_id().clone(),
                result,
            });
            if unresolved || denied {
                state.active_elapsed_ms = elapsed(started, restored_ms);
                observe(host, seed, &step, &observations, &mut state).await?;
                return finish(
                    host,
                    seed,
                    &state,
                    if unresolved {
                        End::Fail(ErrorCode::NeedsReconciliation, Retry::ReconcileFirst)
                    } else {
                        End::Fail(ErrorCode::Denied, Retry::Remediation)
                    },
                )
                .await;
            }
        }
        state.active_elapsed_ms = elapsed(started, restored_ms);
        observe(host, seed, &step, &observations, &mut state).await?;
    }
}
