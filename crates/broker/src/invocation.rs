use crate::{catalog::hash, BrokerError, Effect, SealedCatalog};
use avencrew_contracts::{
    encode_message,
    scalars::{Counter, Digest, DomainId, Instant, JsonObject},
    wire::{
        Lease, NormalizedModelResponseItemsItem, ProtocolVersion, StepBinding, ToolResult,
        TrustedContext, WireMessage,
    },
    Validate,
};
use serde::Serialize;
use std::collections::BTreeSet;

/// Set by the host, never inferred from tool arguments. Supervisor calls use a
/// separately committed synthetic step. PTC must reference an active same-step
/// parent recorded by the host; program execution itself is a later capability.
pub use avencrew_contracts::runtime::Origin;

#[derive(Clone)]
pub struct Invocation {
    pub invocation_id: DomainId,
    pub binding_id: DomainId,
    pub arguments: JsonObject<128>,
    pub origin: Origin,
}

/// Constructed only by the broker after structural/schema/step validation.
/// Serializing it for the journal does not turn its bytes into authority.
#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct PreparedInvocation {
    invocation_id: DomainId,
    context: TrustedContext,
    origin: Origin,
    arguments: JsonObject<128>,
    arguments_digest: Digest,
    adapter_digest: Digest,
    effect: Effect,
}
impl PreparedInvocation {
    pub fn journal_record(
        &self,
    ) -> Result<avencrew_contracts::runtime::LocalProposal, BrokerError> {
        Ok(avencrew_contracts::runtime::LocalProposal {
            invocation_id: self.invocation_id.clone(),
            context: self.context.clone(),
            origin: self.origin.clone(),
            arguments: self.arguments.clone(),
            arguments_digest: self.arguments_digest.clone(),
            adapter_digest: self.adapter_digest.clone(),
            effect: self.effect,
            digest: self.digest()?,
        })
    }
    pub fn invocation_id(&self) -> &DomainId {
        &self.invocation_id
    }
    pub fn context(&self) -> &TrustedContext {
        &self.context
    }
    pub fn origin(&self) -> &Origin {
        &self.origin
    }
    pub fn arguments(&self) -> &JsonObject<128> {
        &self.arguments
    }
    pub fn arguments_digest(&self) -> &Digest {
        &self.arguments_digest
    }
    pub fn adapter_digest(&self) -> &Digest {
        &self.adapter_digest
    }
    pub fn effect(&self) -> Effect {
        self.effect
    }
    /// Versioned normalized typed JSON recipe, supporting finite fractional
    /// arguments. Maps are ordered by serde_json's BTreeMap representation.
    pub fn digest(&self) -> Result<Digest, BrokerError> {
        let bytes = crate::bounds::encode(self, 1_048_576)?;
        let mut input = b"avencrew.invocation/1\0".to_vec();
        input.extend(bytes);
        Ok(hash(&input))
    }
}

/// Current, authenticated host evidence. All policy layers are required;
/// absent grants are false. These snapshots are not accepted from model JSON.
#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct PolicySnapshot {
    pub context: TrustedContext,
    pub lease: Lease,
    pub now: Instant,
    pub boot_id: DomainId,
    pub monotonic_now_ns: Counter,
    pub permission_generation: Counter,
    pub deletion_generation: Counter,
    pub principal_active: bool,
    pub workspace_allowed: bool,
    pub project_allowed: bool,
    pub task_allowed: bool,
    pub worker_allowed: bool,
    pub installation_allowed: bool,
    pub environment_allowed: bool,
    pub target_allowed: bool,
    pub budget_reserved: bool,
    pub dispatch_supported: bool,
    pub cancellation_requested: bool,
    pub unresolved_effect: bool,
    pub parent_active_same_step: bool,
    pub model_call_id: Option<DomainId>,
    pub max_result_bytes: u32,
}
impl PolicySnapshot {
    pub fn digest(&self) -> Result<Digest, BrokerError> {
        // Observation time is not authority state. The host samples its clocks
        // again while claiming and checks deadlines under the authority lock.
        let mut value = serde_json::to_value(self).map_err(|_| BrokerError::StaleAuthority)?;
        let object = value.as_object_mut().ok_or(BrokerError::StaleAuthority)?;
        object.remove("now");
        object.remove("monotonic_now_ns");
        let bytes = serde_json::to_vec(&value).map_err(|_| BrokerError::StaleAuthority)?;
        Ok(hash(&bytes))
    }
}

/// Host commits intent before returning Started. A duplicate intent never
/// yields Started again, even after process loss. Unknown requires reconciliation.
pub enum Dispatch {
    Started,
    Replay(Box<ToolResult>),
    Pending,
    Unknown,
}
pub enum Outcome {
    Result(Box<ToolResult>),
    Pending,
    Unknown,
}

/// Only the broker constructs admission proof. It is borrowed by the trusted
/// host claim, without a clone, serialization or caller-facing return route.
pub struct DispatchAdmission {
    invocation_digest: Digest,
    policy_digest: Digest,
}
impl DispatchAdmission {
    pub fn invocation_digest(&self) -> &Digest {
        &self.invocation_digest
    }
    pub fn policy_digest(&self) -> &Digest {
        &self.policy_digest
    }
}

/// Created only after a committed Started acknowledgment. Preparation alone
/// cannot supply this value to execution or result-recording methods.
pub struct Execution<'a> {
    invocation: &'a PreparedInvocation,
    admission: &'a DispatchAdmission,
}
impl Execution<'_> {
    pub fn invocation(&self) -> &PreparedInvocation {
        self.invocation
    }
    pub fn admission(&self) -> &DispatchAdmission {
        self.admission
    }
}

/// Trusted coordinator port; no implementation is supplied to untrusted code.
/// Storage errors and ambiguous commits fail closed. The host must:
/// - compare exact digests and atomically commit proposal batches (including
///   provider-call uniqueness, parent closure and a previously committed seal);
/// - resolve current policy/target from authoritative state, without using
///   argument fields as grants;
/// - under authority/run locks, recheck the complete snapshot AND lease deadlines
///   and commit one dispatch intent before returning Started;
/// - retain Started/Unknown across restarts and return prior committed results;
/// - execute only the pinned adapter under its lease/generation enforcement.
///
/// No model/tool work occurs inside a database transaction.
///
/// Async methods use static dispatch; no boxed service/runtime is introduced.
#[allow(async_fn_in_trait)]
pub trait Host {
    async fn commit_proposals(
        &mut self,
        step: &StepBinding,
        proposals: &[PreparedInvocation],
    ) -> Result<(), BrokerError>;
    async fn committed_digest(
        &mut self,
        invocation: &PreparedInvocation,
    ) -> Result<Option<Digest>, BrokerError>;
    async fn current_policy(
        &mut self,
        invocation: &PreparedInvocation,
    ) -> Result<PolicySnapshot, BrokerError>;
    async fn claim_dispatch(
        &mut self,
        invocation: &PreparedInvocation,
        admission: &DispatchAdmission,
    ) -> Result<Dispatch, BrokerError>;
    async fn execute(&mut self, execution: &Execution<'_>) -> Result<ToolResult, BrokerError>;
    async fn record_result(
        &mut self,
        execution: &Execution<'_>,
        result: &ToolResult,
    ) -> Result<(), BrokerError>;
    async fn record_unknown(&mut self, execution: &Execution<'_>) -> Result<(), BrokerError>;
}

pub struct Broker {
    step: StepBinding,
    context: TrustedContext,
    catalog: SealedCatalog,
    lease: Lease,
}
impl Broker {
    /// Freeze a host-provided step/context. No mutator or deserialization route
    /// exists for the live broker or its catalog.
    pub fn new(
        step: StepBinding,
        context: TrustedContext,
        lease: Lease,
        catalog: SealedCatalog,
    ) -> Result<Self, BrokerError> {
        step.validate().map_err(|_| BrokerError::InvalidCatalog)?;
        context
            .validate()
            .map_err(|_| BrokerError::StaleAuthority)?;
        lease.validate().map_err(|_| BrokerError::StaleAuthority)?;
        if lease.run_id != context.run_id
            || lease.attempt_id != context.attempt_id
            || lease.epoch != context.epoch
            || lease.environment_id != context.environment_id
            || lease.generation != context.generation
        {
            return Err(BrokerError::StaleAuthority);
        }
        let declared: BTreeSet<_> = step.tool_binding_ids.as_slice().iter().collect();
        let actual: BTreeSet<_> = catalog.bindings().iter().map(|b| &b.binding_id).collect();
        if declared.len() != step.tool_binding_ids.as_slice().len()
            || declared != actual
            || step.catalog_digest != *catalog.digest()
            || context.catalog_digest != *catalog.digest()
            || context.step_id != step.step_id
        {
            return Err(BrokerError::InvalidCatalog);
        }
        Ok(Self {
            step,
            context,
            lease,
            catalog,
        })
    }
    pub fn catalog(&self) -> &SealedCatalog {
        &self.catalog
    }
    pub fn step(&self) -> &StepBinding {
        &self.step
    }
    /// Trusted scope captured with this immutable step (never model arguments).
    pub fn context(&self) -> &TrustedContext {
        &self.context
    }
    pub fn lease(&self) -> &Lease {
        &self.lease
    }

    pub fn prepare(&self, invocation: Invocation) -> Result<PreparedInvocation, BrokerError> {
        use BrokerError::*;
        for value in invocation.arguments.as_map().values() {
            crate::bounds::depth(value)?;
        }
        let arguments = crate::bounds::encode(&invocation.arguments, 262_144)?;
        let value = serde_json::to_value(&invocation.arguments).map_err(|_| InvalidArguments)?;
        let binding = self.catalog.validate(&invocation.binding_id, &value)?;
        match &invocation.origin {
            Origin::Model {
                provider_call_id, ..
            } if provider_call_id.is_empty() || provider_call_id.len() > 256 => {
                return Err(InvalidOrigin)
            }
            Origin::Ptc {
                parent_invocation_id,
            } if parent_invocation_id == &invocation.invocation_id => return Err(InvalidOrigin),
            _ => {}
        }
        // This initial dispatch profile supports read effects only. Actual
        // mutations/approval/containment are admitted by their later consumers.
        if matches!(invocation.origin, Origin::Ptc { .. }) && binding.effect != Effect::Read {
            return Err(Denied);
        }
        let mut context = self.context.clone();
        context.binding_id = invocation.binding_id;
        Ok(PreparedInvocation {
            invocation_id: invocation.invocation_id,
            context,
            origin: invocation.origin,
            arguments: invocation.arguments,
            arguments_digest: hash(&arguments),
            adapter_digest: binding.adapter_digest.clone(),
            effect: binding.effect,
        })
    }

    /// Validate every proposal before committing any of them. A bad item cannot
    /// release the earlier prefix of a model response.
    pub async fn commit<H: Host>(
        &self,
        host: &mut H,
        invocations: Vec<Invocation>,
    ) -> Result<Vec<PreparedInvocation>, BrokerError> {
        if invocations.len() > 100 {
            return Err(BrokerError::LimitExceeded);
        }
        let proposals: Vec<_> = invocations
            .into_iter()
            .map(|i| self.prepare(i))
            .collect::<Result<_, _>>()?;
        let mut ids = BTreeSet::new();
        let mut calls = BTreeSet::new();
        for proposal in &proposals {
            if !ids.insert(proposal.invocation_id()) {
                return Err(BrokerError::IdempotencyMismatch);
            }
            if let Origin::Model {
                provider_call_id, ..
            } = proposal.origin()
            {
                if !calls.insert(provider_call_id) {
                    return Err(BrokerError::IdempotencyMismatch);
                }
            }
        }
        host.commit_proposals(&self.step, &proposals).await?;
        Ok(proposals)
    }

    fn authorize(
        &self,
        invocation: &PreparedInvocation,
        current: &PolicySnapshot,
    ) -> Result<(), BrokerError> {
        use BrokerError::*;
        current.context.validate().map_err(|_| StaleAuthority)?;
        current.lease.validate().map_err(|_| StaleAuthority)?;
        if current.context != invocation.context
            || current.lease.run_id != invocation.context.run_id
            || current.lease.attempt_id != invocation.context.attempt_id
            || current.lease.epoch != invocation.context.epoch
            || current.lease.environment_id != invocation.context.environment_id
            || current.lease.generation != invocation.context.generation
            || current.lease.authority != self.lease.authority
            || current.lease.placement != self.lease.placement
            || current.lease.boot_id != self.lease.boot_id
            || current.lease.boot_id != current.boot_id
            || current.lease.expires_at <= current.now
            || current.lease.monotonic_deadline_ns.value() <= current.monotonic_now_ns.value()
            || current.permission_generation.value() != self.step.permission_generation.value()
            || current.deletion_generation != self.step.deletion_generation
        {
            return Err(StaleAuthority);
        }
        if current.cancellation_requested {
            return Err(Cancelled);
        }
        if current.unresolved_effect {
            return Err(UnknownOutcome);
        }
        if !(1..=262_144).contains(&current.max_result_bytes) {
            return Err(LimitExceeded);
        }
        match &invocation.origin {
            Origin::Model { model_call_id, .. }
                if current.model_call_id.as_ref() != Some(model_call_id) =>
            {
                return Err(InvalidOrigin)
            }
            Origin::Supervisor if current.model_call_id.is_some() => return Err(InvalidOrigin),
            _ => {}
        }
        if !current.dispatch_supported
            || invocation.effect != Effect::Read
            || invocation.context.approval_ref.0.is_some()
        {
            return Err(Unsupported);
        }
        if !current.principal_active
            || !current.workspace_allowed
            || !current.project_allowed
            || !current.task_allowed
            || !current.worker_allowed
            || !current.installation_allowed
            || !current.environment_allowed
            || !current.target_allowed
            || !current.budget_reserved
            || (matches!(invocation.origin, Origin::Ptc { .. }) && !current.parent_active_same_step)
        {
            return Err(Denied);
        }
        Ok(())
    }

    fn validate_result(
        &self,
        invocation: &PreparedInvocation,
        result: &ToolResult,
        max_bytes: u32,
    ) -> Result<(), BrokerError> {
        // The initial read profile has no process/wait owner or durable handle
        // verifier. Do not accept success-shaped claims of live owned work.
        if matches!(
            result,
            ToolResult::Running { .. } | ToolResult::AwaitingInput { .. }
        ) {
            return Err(BrokerError::InvalidResult);
        }
        // Bound before copying a host-supplied result or constructing its envelope.
        match result {
            ToolResult::Succeeded { data, .. } => crate::bounds::depth(&data.value)?,
            ToolResult::Denied { data, .. }
            | ToolResult::Conflict { data, .. }
            | ToolResult::TimedOut { data, .. }
            | ToolResult::Cancelled { data, .. }
            | ToolResult::Lost { data, .. }
            | ToolResult::Unknown { data, .. }
            | ToolResult::Failed { data, .. } => {
                if let Some(data) = &data.0 {
                    crate::bounds::depth(&data.value)?;
                    if data.binding_id != invocation.context.binding_id {
                        return Err(BrokerError::InvalidResult);
                    }
                }
            }
            _ => {}
        }
        crate::bounds::encode(result, max_bytes as usize)?;
        let value = serde_json::to_value(result).map_err(|_| BrokerError::InvalidResult)?;
        if value["invocation_id"] != invocation.invocation_id.as_str() {
            return Err(BrokerError::InvalidResult);
        }
        if let ToolResult::Succeeded { data, .. } = result {
            if data.binding_id != invocation.context.binding_id
                || !self.catalog.validate_output(&data.binding_id, &data.value)
            {
                return Err(BrokerError::InvalidResult);
            }
        }
        encode_message(&WireMessage::ToolResult {
            schema_version: ProtocolVersion::V10,
            payload: Box::new(result.clone()),
        })
        .map_err(|_| BrokerError::InvalidResult)?;
        Ok(())
    }

    /// One entry point for all origins; no authorization token escapes for
    /// later reuse. Replays also need current permission before data release.
    pub async fn invoke<H: Host>(
        &self,
        host: &mut H,
        invocation: Invocation,
    ) -> Result<Outcome, BrokerError> {
        let invocation = self.prepare(invocation)?;
        let committed = host
            .committed_digest(&invocation)
            .await?
            .ok_or(BrokerError::NotCommitted)?;
        if committed != invocation.digest()? {
            return Err(BrokerError::IdempotencyMismatch);
        }
        let current = host.current_policy(&invocation).await?;
        self.authorize(&invocation, &current)?;
        let admission = DispatchAdmission {
            invocation_digest: committed,
            policy_digest: current.digest()?,
        };
        match host.claim_dispatch(&invocation, &admission).await? {
            Dispatch::Replay(result) => {
                let release = host.current_policy(&invocation).await?;
                self.authorize(&invocation, &release)?;
                self.validate_result(&invocation, &result, release.max_result_bytes)?;
                Ok(Outcome::Result(result))
            }
            Dispatch::Pending => Ok(Outcome::Pending),
            Dispatch::Unknown => Ok(Outcome::Unknown),
            Dispatch::Started => {
                let execution = Execution {
                    invocation: &invocation,
                    admission: &admission,
                };
                let result = match host.execute(&execution).await {
                    Ok(result)
                        if self
                            .validate_result(&invocation, &result, current.max_result_bytes)
                            .is_ok() =>
                    {
                        result
                    }
                    _ => {
                        host.record_unknown(&execution)
                            .await
                            .map_err(|_| BrokerError::UnknownOutcome)?;
                        return Ok(Outcome::Unknown);
                    }
                };
                host.record_result(&execution, &result)
                    .await
                    .map_err(|_| BrokerError::UnknownOutcome)?;
                // Completion is retained even if access was revoked while the
                // operation ran. Do not release its data through a stale grant.
                let release = host.current_policy(&invocation).await?;
                self.authorize(&invocation, &release)?;
                self.validate_result(&invocation, &result, release.max_result_bytes)?;
                Ok(Outcome::Result(Box::new(result)))
            }
        }
    }
}

/// Use only complete, normalized model items. Partial provider bytes have no
/// conversion into a broker invocation.
impl Invocation {
    pub fn from_model(
        invocation_id: DomainId,
        model_call_id: DomainId,
        item: &NormalizedModelResponseItemsItem,
    ) -> Result<Self, BrokerError> {
        let NormalizedModelResponseItemsItem::ToolProposal {
            provider_call_id,
            binding_id,
            arguments,
        } = item
        else {
            return Err(BrokerError::InvalidArguments);
        };
        for value in arguments.as_map().values() {
            crate::bounds::depth(value)?;
        }
        crate::bounds::encode(arguments, 262_144)?;
        item.validate().map_err(|_| BrokerError::InvalidArguments)?;
        Ok(Self {
            invocation_id,
            binding_id: binding_id.clone(),
            arguments: arguments.clone(),
            origin: Origin::Model {
                model_call_id,
                provider_call_id: provider_call_id.as_str().into(),
            },
        })
    }
}
