#![allow(dead_code)]
//! Deterministic host-port conformance. The in-memory host models commit cuts;
//! these tests do not establish SQLite durability, guest containment or live PTC.
use avencrew_broker::*;
use avencrew_contracts::{
    scalars::{Digest, DomainId},
    wire::{Lease, StepBinding, ToolResult, TrustedContext},
};
use avencrew_harness::model::fixture::{
    decode_fixture, normalize_fixture, FixtureInput, FixtureResult,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub(crate) fn id(n: usize) -> DomainId {
    DomainId::new(format!("019a5500-0000-7000-8000-{n:012x}")).unwrap()
}
pub(crate) fn digest() -> Digest {
    Digest::new("0".repeat(64)).unwrap()
}
pub(crate) fn run(work: impl std::future::Future<Output = ()>) {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(work);
}
pub(crate) fn fixture(name: &str) -> (FixtureInput, FixtureResult) {
    let catalog: Value =
        serde_json::from_str(include_str!("../../../../fixtures/models/streams.json")).unwrap();
    let events = &catalog["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap()["events"];
    let input = decode_fixture(
        &serde_json::to_vec(&json!({
            "schema_version": catalog["input_schema_version"], "seed": catalog["seed"],
            "events": events,
        }))
        .unwrap(),
    )
    .unwrap();
    let result = normalize_fixture(&input).unwrap();
    (input, result)
}
pub(crate) fn binding(n: usize, name: &str) -> Binding {
    Binding {
        binding_id: id(n),
        name: name.into(),
        capability_version_id: id(n + 1000),
        adapter_digest: digest(),
        effect: Effect::Read,
        input_schema: json!({"type":"object","additionalProperties":false,
            "properties":{"source_name":{"type":"string","minLength":1,"maxLength":64}},
            "required":["source_name"]}),
        output_schema: json!({"type":"object","additionalProperties":false,
            "properties":{"source_name":{"type":"string"},"source_version_id":{"type":"string"}},
            "required":["source_name","source_version_id"]}),
    }
}
pub(crate) fn setup_with(bindings: Vec<Binding>) -> (Broker, FixtureHost) {
    let catalog = SealedCatalog::seal(bindings).unwrap();
    let step: StepBinding = serde_json::from_value(json!({
        "step_id":id(17), "model_profile_id":id(19),"model_adapter_digest":digest(),
        "prompt_schema_version":"1.0","catalog_digest":catalog.digest(),
        "tool_binding_ids":catalog.bindings().iter().map(|b| &b.binding_id).collect::<Vec<_>>(),
        "instruction_digests":[],"context_manifest_id":id(500),
        "environment_capability_digest":digest(),"permission_generation":"1",
        "deletion_generation":"0","sealed_at":"2026-10-08T08:00:00.000000Z",
    }))
    .unwrap();
    let context: TrustedContext = serde_json::from_value(json!({
        "workspace_id":id(1),"project_id":id(6),"run_id":id(100),"attempt_id":id(101),
        "step_id":step.step_id,"principal_id":id(3),"epoch":"1","environment_id":id(102),
        "generation":"1","binding_id":id(18),"catalog_digest":catalog.digest(),
        "budget_hold_refs":[id(103)],"approval_ref":null,
    }))
    .unwrap();
    let lease: Lease = serde_json::from_value(json!({
        "run_id":context.run_id,"attempt_id":context.attempt_id,"epoch":"1",
        "placement":"local","authority":"local","environment_id":context.environment_id,
        "generation":"1","expires_at":"2026-10-08T09:00:00.000000Z","boot_id":id(104),
        "monotonic_deadline_ns":"100000","receipt_digest":digest(),
    }))
    .unwrap();
    let policy = PolicySnapshot {
        context: context.clone(),
        lease: lease.clone(),
        now: serde_json::from_value(json!("2026-10-08T08:00:01.000000Z")).unwrap(),
        boot_id: id(104),
        monotonic_now_ns: serde_json::from_value(json!("10")).unwrap(),
        permission_generation: serde_json::from_value(json!("1")).unwrap(),
        deletion_generation: serde_json::from_value(json!("0")).unwrap(),
        principal_active: true,
        workspace_allowed: true,
        project_allowed: true,
        task_allowed: true,
        worker_allowed: true,
        installation_allowed: true,
        environment_allowed: true,
        target_allowed: true,
        budget_reserved: true,
        dispatch_supported: true,
        cancellation_requested: false,
        unresolved_effect: false,
        parent_active_same_step: true,
        model_call_id: Some(id(16)),
        max_result_bytes: 32768,
    };
    (
        Broker::new(step.clone(), context, lease, catalog).unwrap(),
        FixtureHost::new(policy, step),
    )
}
pub(crate) fn setup() -> (Broker, FixtureHost) {
    setup_with(vec![binding(18, "workspace_read")])
}
pub(crate) fn invocation(n: usize, origin: Origin) -> Invocation {
    Invocation {
        invocation_id: id(n),
        binding_id: id(18),
        arguments: serde_json::from_value(json!({"source_name":"account_snapshot"})).unwrap(),
        origin,
    }
}
pub(crate) fn model(n: usize) -> Invocation {
    invocation(
        n,
        Origin::Model {
            model_call_id: id(16),
            provider_call_id: format!("call-{n}"),
        },
    )
}

#[derive(Clone)]
pub(crate) enum Retained {
    Proposed,
    Started,
    Result(Box<ToolResult>),
    Unknown,
}
#[derive(Clone)]
pub(crate) struct FixtureHost {
    pub(crate) policy: PolicySnapshot,
    pub(crate) steps: BTreeMap<DomainId, StepBinding>,
    pub(crate) rows: BTreeMap<DomainId, (Digest, Retained)>,
    pub(crate) calls: BTreeMap<(DomainId, String), DomainId>,
    pub(crate) dispatch_count: usize,
    pub(crate) commit_count: usize,
    pub(crate) revoke_at_claim: bool,
    pub(crate) revoke_at_release: bool,
    pub(crate) fail_commit: bool,
    pub(crate) fail_claim: bool,
    pub(crate) fail_claim_reply: bool,
    pub(crate) claim_clock: Option<u64>,
    pub(crate) fail_result_commit: bool,
    pub(crate) fail_execute: bool,
    pub(crate) result_override: Option<ToolResult>,
}
impl FixtureHost {
    pub(crate) fn new(policy: PolicySnapshot, step: StepBinding) -> Self {
        Self {
            policy,
            steps: BTreeMap::from([(step.step_id.clone(), step)]),
            rows: BTreeMap::new(),
            calls: BTreeMap::new(),
            dispatch_count: 0,
            commit_count: 0,
            revoke_at_claim: false,
            revoke_at_release: false,
            fail_commit: false,
            fail_claim: false,
            fail_claim_reply: false,
            claim_clock: None,
            fail_result_commit: false,
            fail_execute: false,
            result_override: None,
        }
    }
    fn snapshot(&self, proposal: &PreparedInvocation) -> PolicySnapshot {
        let mut policy = self.policy.clone();
        policy.context.binding_id = proposal.context().binding_id.clone();
        let sources: Value =
            serde_json::from_str(include_str!("../../../../fixtures/sources/evidence.json"))
                .unwrap();
        let source = sources["sources"]
            .as_array()
            .unwrap()
            .iter()
            .find(|source| source["name"] == proposal.arguments().as_map()["source_name"]);
        policy.target_allowed &= source.is_some_and(|source| {
            source["workspace_id"] == proposal.context().workspace_id.as_str()
                && source["audience_ids"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(proposal.context().principal_id))
        });
        policy
    }
    pub(crate) fn success(&self, proposal: &PreparedInvocation) -> ToolResult {
        let sources: Value =
            serde_json::from_str(include_str!("../../../../fixtures/sources/evidence.json"))
                .unwrap();
        let source = sources["sources"]
            .as_array()
            .unwrap()
            .iter()
            .find(|source| source["name"] == proposal.arguments().as_map()["source_name"])
            .unwrap();
        serde_json::from_value(json!({
            "status":"succeeded","invocation_id":proposal.invocation_id(),"schema_version":"1.0",
            "artifact_refs":[],"receipt_ref":null,"truncated":false,"next_cursor":null,"usage_refs":[],
            "data":{"binding_id":proposal.context().binding_id,
                "value":{"source_name":source["name"],"source_version_id":source["version_id"]}},
        })).unwrap()
    }
}
impl Host for FixtureHost {
    async fn commit_proposals(
        &mut self,
        step: &StepBinding,
        proposals: &[PreparedInvocation],
    ) -> Result<(), BrokerError> {
        if self.steps.get(&step.step_id) != Some(step) {
            return Err(BrokerError::InvalidCatalog);
        }
        let mut rows = self.rows.clone();
        let mut calls = self.calls.clone();
        for proposal in proposals {
            let digest = proposal.digest()?;
            if rows
                .get(proposal.invocation_id())
                .is_some_and(|(old, _)| old != &digest)
            {
                return Err(BrokerError::IdempotencyMismatch);
            }
            if let Origin::Model {
                provider_call_id, ..
            } = proposal.origin()
            {
                let key = (step.step_id.clone(), provider_call_id.clone());
                if calls
                    .get(&key)
                    .is_some_and(|id| id != proposal.invocation_id())
                {
                    return Err(BrokerError::IdempotencyMismatch);
                }
                calls.insert(key, proposal.invocation_id().clone());
            }
            rows.entry(proposal.invocation_id().clone())
                .or_insert((digest, Retained::Proposed));
        }
        if self.fail_commit {
            return Err(BrokerError::Storage);
        }
        self.rows = rows;
        self.calls = calls;
        self.commit_count += 1;
        Ok(())
    }
    async fn committed_digest(
        &mut self,
        invocation: &PreparedInvocation,
    ) -> Result<Option<Digest>, BrokerError> {
        Ok(self
            .rows
            .get(invocation.invocation_id())
            .map(|(digest, _)| digest.clone()))
    }
    async fn current_policy(
        &mut self,
        invocation: &PreparedInvocation,
    ) -> Result<PolicySnapshot, BrokerError> {
        Ok(self.snapshot(invocation))
    }
    async fn claim_dispatch(
        &mut self,
        invocation: &PreparedInvocation,
        admission: &DispatchAdmission,
    ) -> Result<Dispatch, BrokerError> {
        if self.revoke_at_claim {
            self.policy.target_allowed = false;
        }
        if self.fail_claim {
            return Err(BrokerError::Storage);
        }
        if let Some(clock) = self.claim_clock {
            self.policy.monotonic_now_ns =
                avencrew_contracts::scalars::Counter::new(clock.to_string()).unwrap();
        }
        let current = self.snapshot(invocation);
        if current.digest()? != *admission.policy_digest()
            || current.lease.expires_at <= current.now
            || current.lease.monotonic_deadline_ns.value() <= current.monotonic_now_ns.value()
        {
            return Err(BrokerError::StaleAuthority);
        }
        let (digest, state) = self
            .rows
            .get_mut(invocation.invocation_id())
            .ok_or(BrokerError::NotCommitted)?;
        if *digest != invocation.digest()? || digest != admission.invocation_digest() {
            return Err(BrokerError::IdempotencyMismatch);
        }
        match state {
            Retained::Proposed => {
                *state = Retained::Started;
                if self.fail_claim_reply {
                    Err(BrokerError::UnknownOutcome)
                } else {
                    Ok(Dispatch::Started)
                }
            }
            Retained::Started => Ok(Dispatch::Pending),
            Retained::Result(result) => Ok(Dispatch::Replay(result.clone())),
            Retained::Unknown => Ok(Dispatch::Unknown),
        }
    }
    async fn execute(&mut self, execution: &Execution<'_>) -> Result<ToolResult, BrokerError> {
        let invocation = execution.invocation();
        assert!(matches!(
            self.rows[invocation.invocation_id()].1,
            Retained::Started
        ));
        self.dispatch_count += 1;
        if self.revoke_at_release {
            self.policy.target_allowed = false;
        }
        if self.fail_execute {
            return Err(BrokerError::UnknownOutcome);
        }
        Ok(self
            .result_override
            .clone()
            .unwrap_or_else(|| self.success(invocation)))
    }
    async fn record_result(
        &mut self,
        execution: &Execution<'_>,
        result: &ToolResult,
    ) -> Result<(), BrokerError> {
        let invocation = execution.invocation();
        if self.fail_result_commit {
            return Err(BrokerError::Storage);
        }
        self.rows.get_mut(invocation.invocation_id()).unwrap().1 =
            Retained::Result(Box::new(result.clone()));
        Ok(())
    }
    async fn record_unknown(&mut self, execution: &Execution<'_>) -> Result<(), BrokerError> {
        let invocation = execution.invocation();
        self.rows.get_mut(invocation.invocation_id()).unwrap().1 = Retained::Unknown;
        Ok(())
    }
}
