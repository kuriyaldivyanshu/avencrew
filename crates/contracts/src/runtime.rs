//! Internal portable execution records, shared by the harness and journal.
//! These records contain no provider transport or database implementation.
use crate::{
    scalars::{Counter, Digest, DomainId},
    wire::NormalizedModelResponse,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactFormat {
    #[default]
    Text,
    Json,
    Csv,
    Patch,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Passed,
    Failed,
    Unknown,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactValidation {
    pub schema_version: String,
    pub format: ArtifactFormat,
    pub sha256: Digest,
    pub byte_size: Counter,
    pub status: CheckStatus,
    pub codes: Vec<String>,
    pub base_sha256: Option<Digest>,
    pub result_sha256: Option<Digest>,
}
pub fn content_digest(bytes: &[u8]) -> Digest {
    use sha2::{Digest as _, Sha256};
    Digest::new(
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>(),
    )
    .expect("SHA-256 encoding")
}

/// Exact sealed inference input. Only the trusted coordinator creates this.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalStep {
    pub accepted_input_cut: Counter,
    pub candidate: Option<CandidateAllocation>,
    pub binding: crate::wire::StepBinding,
    pub context: crate::wire::TrustedContext,
    pub lease: crate::wire::Lease,
    pub bindings: Vec<Binding>,
    pub model_call_id: DomainId,
    pub request_digest: Digest,
    pub provider_profile_id: DomainId,
    pub compatibility_digest: Digest,
    pub request: Value,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateAllocation {
    pub artifact_id: DomainId,
    pub version_id: DomainId,
    pub content_blob_id: DomainId,
    pub checks_manifest_id: DomainId,
    pub checks_blob_id: DomainId,
    pub check_result_id: DomainId,
}

/// Retained counters and exact pointers; the sealed records remain authoritative.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalTurnProgress {
    pub state: TurnState,
    pub pending_step_id: Option<DomainId>,
    pub checkpoint_id: Option<DomainId>,
    pub outcome: Option<crate::wire::TurnOutcome>,
}

pub type ModelCompletion = Result<FixtureResult, FixtureFailure>;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalProposal {
    pub invocation_id: DomainId,
    pub context: crate::wire::TrustedContext,
    pub origin: Origin,
    pub arguments: crate::scalars::JsonObject<128>,
    pub arguments_digest: Digest,
    pub adapter_digest: Digest,
    pub effect: Effect,
    pub digest: Digest,
}

/// Versioned normalized JSON for complete model/tool data, including finite
/// fractional arguments. This is distinct from canonical integral domain records.
pub fn record_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, crate::ContractError> {
    struct Bounded(Vec<u8>);
    impl std::io::Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.0.len().saturating_add(bytes.len()) > 1_048_576 {
                return Err(std::io::Error::other("runtime record bound"));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut sink = Bounded(Vec::new());
    serde_json::to_writer(&mut sink, value)
        .map_err(|_| crate::ContractError::Limit("runtime record bytes"))?;
    Ok(sink.0)
}

impl LocalProposal {
    pub fn calculated_digest(&self) -> Result<Digest, crate::ContractError> {
        use crate::Validate;
        use sha2::{Digest as _, Sha256};
        #[derive(Serialize)]
        struct Material<'a> {
            invocation_id: &'a DomainId,
            context: &'a crate::wire::TrustedContext,
            origin: &'a Origin,
            arguments: &'a crate::scalars::JsonObject<128>,
            arguments_digest: &'a Digest,
            adapter_digest: &'a Digest,
            effect: Effect,
        }
        self.context.validate()?;
        self.arguments.validate()?;
        let arguments = Sha256::digest(record_bytes(&self.arguments)?);
        let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
        if hex(&arguments) != self.arguments_digest.as_str() {
            return Err(crate::ContractError::InvalidMessage);
        }
        let mut bytes = b"avencrew.invocation/1\0".to_vec();
        bytes.extend(record_bytes(&Material {
            invocation_id: &self.invocation_id,
            context: &self.context,
            origin: &self.origin,
            arguments: &self.arguments,
            arguments_digest: &self.arguments_digest,
            adapter_digest: &self.adapter_digest,
            effect: self.effect,
        })?);
        Ok(content_digest(&bytes))
    }
}
impl crate::Validate for LocalProposal {
    fn validate(&self) -> Result<(), crate::ContractError> {
        if self.calculated_digest()? != self.digest {
            return Err(crate::ContractError::InvalidMessage);
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum FixtureUsage {
    Unknown,
    Known {
        input_tokens: Counter,
        output_tokens: Counter,
    },
}
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProposalIdentity {
    pub item: u32,
    pub provider_call_id: String,
    pub invocation_id: DomainId,
}
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FixtureResult {
    pub response: NormalizedModelResponse,
    pub usage: FixtureUsage,
    pub proposal_identities: Vec<ProposalIdentity>,
}
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FixtureFailure {
    pub code: FixtureError,
    pub usage: FixtureUsage,
}
#[derive(Debug, Copy, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FixtureError {
    Cancelled,
    Timeout,
    InvalidInput,
    IncompatibleVersion,
    LimitExceeded,
    InvalidSequence,
    ReplayMismatch,
    InvalidItemState,
    DuplicateCall,
    UnknownBinding,
    InvalidUtf8,
    InvalidArguments,
    IncompatibleContinuation,
    InvalidFinish,
    InvalidResponse,
}
impl fmt::Display for FixtureFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.code)
    }
}
impl fmt::Debug for FixtureFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
impl std::error::Error for FixtureFailure {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    Read,
    IsolatedMutation,
    SharedMutation,
    ExternalEffect,
    PrivilegedExecution,
}

/// Trusted descriptor. Selecting a descriptor never grants permission.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub binding_id: DomainId,
    pub name: String,
    pub capability_version_id: DomainId,
    pub adapter_digest: Digest,
    pub effect: Effect,
    pub input_schema: Value,
    pub output_schema: Value,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Origin {
    Model {
        model_call_id: DomainId,
        provider_call_id: String,
    },
    Supervisor,
    Ptc {
        parent_invocation_id: DomainId,
    },
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnState {
    pub steps_started: u32,
    pub no_progress_steps: u32,
    pub active_elapsed_ms: u64,
    pub applied_input_cut: Counter,
    pub progress_digest: Digest,
}
