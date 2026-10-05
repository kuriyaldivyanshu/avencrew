//! Canonical protocol 1.0 DTOs. Reviewed from the design schema; Rust owns
//! subsequent changes. Opaque tool data still requires sealed-binding validation.
use crate::{scalars::*, ContractError, Validate};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct ProcessHandle {
    pub environment_id: DomainId,
    pub generation: PositiveCounter,
    pub handle: Text<0, 256>,
}
impl Validate for ProcessHandle {
    fn validate(&self) -> Result<(), ContractError> {
        self.environment_id.validate()?;
        self.generation.validate()?;
        self.handle.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum ErrorCode {
    #[serde(rename = "INVALID_MESSAGE")]
    InvalidMessage,
    #[serde(rename = "INVALID_ARGUMENT")]
    InvalidArgument,
    #[serde(rename = "UNSUPPORTED")]
    Unsupported,
    #[serde(rename = "INCOMPATIBLE")]
    Incompatible,
    #[serde(rename = "UNAUTHENTICATED")]
    Unauthenticated,
    #[serde(rename = "DENIED")]
    Denied,
    #[serde(rename = "NOT_FOUND_OR_DENIED")]
    NotFoundOrDenied,
    #[serde(rename = "CONFLICT")]
    Conflict,
    #[serde(rename = "IDEMPOTENCY_MISMATCH")]
    IdempotencyMismatch,
    #[serde(rename = "STALE_AUTHORITY")]
    StaleAuthority,
    #[serde(rename = "CANCELLED")]
    Cancelled,
    #[serde(rename = "TIMEOUT")]
    Timeout,
    #[serde(rename = "LOST")]
    Lost,
    #[serde(rename = "UNKNOWN_OUTCOME")]
    UnknownOutcome,
    #[serde(rename = "RESOURCE_LIMIT")]
    ResourceLimit,
    #[serde(rename = "IO_FAILURE")]
    IoFailure,
    #[serde(rename = "RATE_LIMITED")]
    RateLimited,
    #[serde(rename = "UNAVAILABLE")]
    Unavailable,
    #[serde(rename = "NEEDS_RECONCILIATION")]
    NeedsReconciliation,
}
impl Validate for ErrorCode {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct SafeError {
    pub code: ErrorCode,
    pub message: Text<0, 1024>,
    pub evidence_refs: List<DomainId, 0, 100>,
}
impl Validate for SafeError {
    fn validate(&self) -> Result<(), ContractError> {
        self.code.validate()?;
        self.message.validate()?;
        self.evidence_refs.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum Authority {
    #[serde(rename = "local")]
    Local,
    #[serde(rename = "server")]
    Server,
    #[serde(rename = "transfer_staging")]
    TransferStaging,
}
impl Validate for Authority {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum PendingReason {
    #[serde(rename = "unsent")]
    Unsent,
    #[serde(rename = "unacknowledged")]
    Unacknowledged,
    #[serde(rename = "partitioned")]
    Partitioned,
}
impl Validate for PendingReason {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum TerminalRunState {
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "cancelled")]
    Cancelled,
    #[serde(rename = "failed")]
    Failed,
}
impl Validate for TerminalRunState {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(tag = "kind")]
#[serde(deny_unknown_fields)]
pub enum CommandReceipt {
    #[serde(rename = "pending")]
    Pending {
        command_id: DomainId,
        authority: Authority,
        reason: PendingReason,
    },
    #[serde(rename = "accepted")]
    Accepted {
        command_id: DomainId,
        authority: Authority,
        run_id: DomainId,
        accepted_seq: Counter,
        event_seq: Counter,
        row_version: PositiveCounter,
        disposition_ref: Nullable<DomainId>,
        task_id: DomainId,
        task_revision_id: DomainId,
    },
    #[serde(rename = "applied")]
    Applied {
        command_id: DomainId,
        authority: Authority,
        run_id: DomainId,
        accepted_seq: Counter,
        event_seq: Counter,
        row_version: PositiveCounter,
        disposition_ref: DomainId,
        task_id: DomainId,
        task_revision_id: DomainId,
    },
    #[serde(rename = "deferred")]
    Deferred {
        command_id: DomainId,
        authority: Authority,
        run_id: DomainId,
        accepted_seq: Counter,
        event_seq: Counter,
        row_version: PositiveCounter,
        disposition_ref: DomainId,
        task_id: DomainId,
        task_revision_id: DomainId,
    },
    #[serde(rename = "superseded")]
    Superseded {
        command_id: DomainId,
        authority: Authority,
        run_id: DomainId,
        accepted_seq: Counter,
        event_seq: Counter,
        row_version: PositiveCounter,
        disposition_ref: DomainId,
        task_id: DomainId,
        task_revision_id: DomainId,
    },
    #[serde(rename = "rejected")]
    Rejected {
        command_id: DomainId,
        run_id: DomainId,
        error: SafeError,
        current_revision: Nullable<Counter>,
    },
    #[serde(rename = "conflict")]
    Conflict {
        command_id: DomainId,
        run_id: DomainId,
        error: SafeError,
        current_revision: Nullable<Counter>,
    },
    #[serde(rename = "terminal")]
    Terminal {
        command_id: DomainId,
        run_id: DomainId,
        state: TerminalRunState,
        row_version: PositiveCounter,
    },
}
impl Validate for CommandReceipt {
    fn validate(&self) -> Result<(), ContractError> {
        match self {
            Self::Pending {
                command_id,
                authority,
                reason,
            } => {
                command_id.validate()?;
                authority.validate()?;
                reason.validate()?;
                Ok(())
            }
            Self::Accepted {
                command_id,
                authority,
                run_id,
                accepted_seq,
                event_seq,
                row_version,
                disposition_ref,
                task_id,
                task_revision_id,
            } => {
                command_id.validate()?;
                authority.validate()?;
                run_id.validate()?;
                accepted_seq.validate()?;
                event_seq.validate()?;
                row_version.validate()?;
                disposition_ref.validate()?;
                task_id.validate()?;
                task_revision_id.validate()?;
                Ok(())
            }
            Self::Applied {
                command_id,
                authority,
                run_id,
                accepted_seq,
                event_seq,
                row_version,
                disposition_ref,
                task_id,
                task_revision_id,
            } => {
                command_id.validate()?;
                authority.validate()?;
                run_id.validate()?;
                accepted_seq.validate()?;
                event_seq.validate()?;
                row_version.validate()?;
                disposition_ref.validate()?;
                task_id.validate()?;
                task_revision_id.validate()?;
                Ok(())
            }
            Self::Deferred {
                command_id,
                authority,
                run_id,
                accepted_seq,
                event_seq,
                row_version,
                disposition_ref,
                task_id,
                task_revision_id,
            } => {
                command_id.validate()?;
                authority.validate()?;
                run_id.validate()?;
                accepted_seq.validate()?;
                event_seq.validate()?;
                row_version.validate()?;
                disposition_ref.validate()?;
                task_id.validate()?;
                task_revision_id.validate()?;
                Ok(())
            }
            Self::Superseded {
                command_id,
                authority,
                run_id,
                accepted_seq,
                event_seq,
                row_version,
                disposition_ref,
                task_id,
                task_revision_id,
            } => {
                command_id.validate()?;
                authority.validate()?;
                run_id.validate()?;
                accepted_seq.validate()?;
                event_seq.validate()?;
                row_version.validate()?;
                disposition_ref.validate()?;
                task_id.validate()?;
                task_revision_id.validate()?;
                Ok(())
            }
            Self::Rejected {
                command_id,
                run_id,
                error,
                current_revision,
            } => {
                command_id.validate()?;
                run_id.validate()?;
                error.validate()?;
                current_revision.validate()?;
                Ok(())
            }
            Self::Conflict {
                command_id,
                run_id,
                error,
                current_revision,
            } => {
                command_id.validate()?;
                run_id.validate()?;
                error.validate()?;
                current_revision.validate()?;
                Ok(())
            }
            Self::Terminal {
                command_id,
                run_id,
                state,
                row_version,
            } => {
                command_id.validate()?;
                run_id.validate()?;
                state.validate()?;
                row_version.validate()?;
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct ArtifactRef {
    pub artifact_id: DomainId,
    pub version_id: DomainId,
    pub sha256: Digest,
    pub byte_size: Counter,
}
impl Validate for ArtifactRef {
    fn validate(&self) -> Result<(), ContractError> {
        self.artifact_id.validate()?;
        self.version_id.validate()?;
        self.sha256.validate()?;
        self.byte_size.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum Placement {
    #[serde(rename = "local")]
    Local,
    #[serde(rename = "cloud")]
    Cloud,
}
impl Validate for Placement {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum ExecutionAuthority {
    #[serde(rename = "local")]
    Local,
    #[serde(rename = "server")]
    Server,
}
impl Validate for ExecutionAuthority {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct Lease {
    pub run_id: DomainId,
    pub attempt_id: DomainId,
    pub epoch: PositiveCounter,
    pub placement: Placement,
    pub authority: ExecutionAuthority,
    pub environment_id: Nullable<DomainId>,
    pub generation: Nullable<PositiveCounter>,
    pub expires_at: Instant,
    pub boot_id: DomainId,
    pub monotonic_deadline_ns: Counter,
    pub receipt_digest: Digest,
}
impl Validate for Lease {
    fn validate(&self) -> Result<(), ContractError> {
        self.run_id.validate()?;
        self.attempt_id.validate()?;
        self.epoch.validate()?;
        self.placement.validate()?;
        self.authority.validate()?;
        self.environment_id.validate()?;
        self.generation.validate()?;
        self.expires_at.validate()?;
        self.boot_id.validate()?;
        self.monotonic_deadline_ns.validate()?;
        self.receipt_digest.validate()?;
        crate::semantics::lease(self)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum ProtocolVersion {
    #[serde(rename = "1.0")]
    V10,
}
impl Validate for ProtocolVersion {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct StepBinding {
    pub step_id: DomainId,
    pub model_profile_id: DomainId,
    pub model_adapter_digest: Digest,
    pub prompt_schema_version: ProtocolVersion,
    pub catalog_digest: Digest,
    pub tool_binding_ids: List<DomainId, 0, 1000>,
    pub instruction_digests: List<Digest, 0, 100>,
    pub context_manifest_id: DomainId,
    pub environment_capability_digest: Digest,
    pub permission_generation: PositiveCounter,
    pub deletion_generation: Counter,
    pub sealed_at: Instant,
}
impl Validate for StepBinding {
    fn validate(&self) -> Result<(), ContractError> {
        self.step_id.validate()?;
        self.model_profile_id.validate()?;
        self.model_adapter_digest.validate()?;
        self.prompt_schema_version.validate()?;
        self.catalog_digest.validate()?;
        self.tool_binding_ids.validate()?;
        self.instruction_digests.validate()?;
        self.context_manifest_id.validate()?;
        self.environment_capability_digest.validate()?;
        self.permission_generation.validate()?;
        self.deletion_generation.validate()?;
        self.sealed_at.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct TrustedContext {
    pub workspace_id: DomainId,
    pub project_id: Nullable<DomainId>,
    pub run_id: DomainId,
    pub attempt_id: DomainId,
    pub step_id: DomainId,
    pub principal_id: DomainId,
    pub epoch: PositiveCounter,
    pub environment_id: Nullable<DomainId>,
    pub generation: Nullable<PositiveCounter>,
    pub binding_id: DomainId,
    pub catalog_digest: Digest,
    pub budget_hold_refs: List<DomainId, 0, 100>,
    pub approval_ref: Nullable<DomainId>,
}
impl Validate for TrustedContext {
    fn validate(&self) -> Result<(), ContractError> {
        self.workspace_id.validate()?;
        self.project_id.validate()?;
        self.run_id.validate()?;
        self.attempt_id.validate()?;
        self.step_id.validate()?;
        self.principal_id.validate()?;
        self.epoch.validate()?;
        self.environment_id.validate()?;
        self.generation.validate()?;
        self.binding_id.validate()?;
        self.catalog_digest.validate()?;
        self.budget_hold_refs.validate()?;
        self.approval_ref.validate()?;
        crate::semantics::trusted_context(self)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RunSpec {
    pub task_id: DomainId,
    pub task_revision_id: DomainId,
    pub run_id: DomainId,
    pub session_id: DomainId,
    pub branch_id: DomainId,
    pub workspace_id: DomainId,
    pub project_id: Nullable<DomainId>,
    pub worker_id: Nullable<DomainId>,
    pub responsibility_id: Nullable<DomainId>,
    pub requester_id: DomainId,
    pub owner_id: DomainId,
    pub acting_principal_id: DomainId,
    pub audience_policy_id: DomainId,
    pub execution_policy_id: DomainId,
    pub objective_version_id: DomainId,
    pub check_refs: List<DomainId, 0, 100>,
    pub instruction_digests: List<Digest, 0, 100>,
    pub model_profile_id: DomainId,
    pub environment_recipe_version_id: Nullable<DomainId>,
    pub budget_period_refs: List<DomainId, 0, 100>,
    pub deadline: Nullable<Instant>,
    pub placement: Placement,
    pub accepted_input_seq: Counter,
}
impl Validate for RunSpec {
    fn validate(&self) -> Result<(), ContractError> {
        self.task_id.validate()?;
        self.task_revision_id.validate()?;
        self.run_id.validate()?;
        self.session_id.validate()?;
        self.branch_id.validate()?;
        self.workspace_id.validate()?;
        self.project_id.validate()?;
        self.worker_id.validate()?;
        self.responsibility_id.validate()?;
        self.requester_id.validate()?;
        self.owner_id.validate()?;
        self.acting_principal_id.validate()?;
        self.audience_policy_id.validate()?;
        self.execution_policy_id.validate()?;
        self.objective_version_id.validate()?;
        self.check_refs.validate()?;
        self.instruction_digests.validate()?;
        self.model_profile_id.validate()?;
        self.environment_recipe_version_id.validate()?;
        self.budget_period_refs.validate()?;
        self.deadline.validate()?;
        self.placement.validate()?;
        self.accepted_input_seq.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct CommandSteerPayload {
    pub text: Text<0, 8192>,
    pub expected_revision: PositiveCounter,
}
impl Validate for CommandSteerPayload {
    fn validate(&self) -> Result<(), ContractError> {
        self.text.validate()?;
        self.expected_revision.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct CommandReviseObjectivePayload {
    pub objective_version_id: DomainId,
    pub check_refs: List<DomainId, 0, 100>,
    pub expected_revision: PositiveCounter,
}
impl Validate for CommandReviseObjectivePayload {
    fn validate(&self) -> Result<(), ContractError> {
        self.objective_version_id.validate()?;
        self.check_refs.validate()?;
        self.expected_revision.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct CommandPausePayload {
    pub expected_revision: PositiveCounter,
    pub reason: Text<0, 1024>,
}
impl Validate for CommandPausePayload {
    fn validate(&self) -> Result<(), ContractError> {
        self.expected_revision.validate()?;
        self.reason.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct CommandResumePayload {
    pub expected_revision: PositiveCounter,
    pub checkpoint_id: DomainId,
}
impl Validate for CommandResumePayload {
    fn validate(&self) -> Result<(), ContractError> {
        self.expected_revision.validate()?;
        self.checkpoint_id.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct CommandStopPayload {
    pub reason: Text<0, 1024>,
}
impl Validate for CommandStopPayload {
    fn validate(&self) -> Result<(), ContractError> {
        self.reason.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct CommandAnswerClarificationPayloadAnswersItem {
    pub question_id: Text<0, 128>,
    pub answer: Text<0, 8192>,
}
impl Validate for CommandAnswerClarificationPayloadAnswersItem {
    fn validate(&self) -> Result<(), ContractError> {
        self.question_id.validate()?;
        self.answer.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct CommandAnswerClarificationPayload {
    pub request_id: DomainId,
    pub request_revision: PositiveCounter,
    pub answers: List<CommandAnswerClarificationPayloadAnswersItem, 0, 3>,
}
impl Validate for CommandAnswerClarificationPayload {
    fn validate(&self) -> Result<(), ContractError> {
        self.request_id.validate()?;
        self.request_revision.validate()?;
        self.answers.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum CommandReviewArtifactPayloadResponse {
    #[serde(rename = "accept")]
    Accept,
    #[serde(rename = "reject")]
    Reject,
    #[serde(rename = "revise")]
    Revise,
}
impl Validate for CommandReviewArtifactPayloadResponse {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct CommandReviewArtifactPayload {
    pub request_id: DomainId,
    pub request_revision: PositiveCounter,
    pub artifact_id: DomainId,
    pub version_id: DomainId,
    pub check_refs: List<DomainId, 0, 100>,
    pub response: CommandReviewArtifactPayloadResponse,
}
impl Validate for CommandReviewArtifactPayload {
    fn validate(&self) -> Result<(), ContractError> {
        self.request_id.validate()?;
        self.request_revision.validate()?;
        self.artifact_id.validate()?;
        self.version_id.validate()?;
        self.check_refs.validate()?;
        self.response.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum CommandDecideActionPayloadResponse {
    #[serde(rename = "approve")]
    Approve,
    #[serde(rename = "reject")]
    Reject,
}
impl Validate for CommandDecideActionPayloadResponse {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct CommandDecideActionPayload {
    pub decision_id: DomainId,
    pub decision_revision: PositiveCounter,
    pub invocation_id: DomainId,
    pub target_digest: Digest,
    pub payload_digest: Digest,
    pub response: CommandDecideActionPayloadResponse,
}
impl Validate for CommandDecideActionPayload {
    fn validate(&self) -> Result<(), ContractError> {
        self.decision_id.validate()?;
        self.decision_revision.validate()?;
        self.invocation_id.validate()?;
        self.target_digest.validate()?;
        self.payload_digest.validate()?;
        self.response.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(tag = "kind")]
#[serde(deny_unknown_fields)]
pub enum Command {
    #[serde(rename = "steer")]
    Steer {
        command_id: DomainId,
        run_id: DomainId,
        idempotency_key: Text<1, 128>,
        payload: CommandSteerPayload,
    },
    #[serde(rename = "revise_objective")]
    ReviseObjective {
        command_id: DomainId,
        run_id: DomainId,
        idempotency_key: Text<1, 128>,
        payload: CommandReviseObjectivePayload,
    },
    #[serde(rename = "pause")]
    Pause {
        command_id: DomainId,
        run_id: DomainId,
        idempotency_key: Text<1, 128>,
        payload: CommandPausePayload,
    },
    #[serde(rename = "resume")]
    Resume {
        command_id: DomainId,
        run_id: DomainId,
        idempotency_key: Text<1, 128>,
        payload: CommandResumePayload,
    },
    #[serde(rename = "stop")]
    Stop {
        command_id: DomainId,
        run_id: DomainId,
        idempotency_key: Text<1, 128>,
        payload: CommandStopPayload,
    },
    #[serde(rename = "answer_clarification")]
    AnswerClarification {
        command_id: DomainId,
        run_id: DomainId,
        idempotency_key: Text<1, 128>,
        payload: CommandAnswerClarificationPayload,
    },
    #[serde(rename = "review_artifact")]
    ReviewArtifact {
        command_id: DomainId,
        run_id: DomainId,
        idempotency_key: Text<1, 128>,
        payload: CommandReviewArtifactPayload,
    },
    #[serde(rename = "decide_action")]
    DecideAction {
        command_id: DomainId,
        run_id: DomainId,
        idempotency_key: Text<1, 128>,
        payload: CommandDecideActionPayload,
    },
}
impl Validate for Command {
    fn validate(&self) -> Result<(), ContractError> {
        match self {
            Self::Steer {
                command_id,
                run_id,
                idempotency_key,
                payload,
            } => {
                command_id.validate()?;
                run_id.validate()?;
                idempotency_key.validate()?;
                payload.validate()?;
                Ok(())
            }
            Self::ReviseObjective {
                command_id,
                run_id,
                idempotency_key,
                payload,
            } => {
                command_id.validate()?;
                run_id.validate()?;
                idempotency_key.validate()?;
                payload.validate()?;
                Ok(())
            }
            Self::Pause {
                command_id,
                run_id,
                idempotency_key,
                payload,
            } => {
                command_id.validate()?;
                run_id.validate()?;
                idempotency_key.validate()?;
                payload.validate()?;
                Ok(())
            }
            Self::Resume {
                command_id,
                run_id,
                idempotency_key,
                payload,
            } => {
                command_id.validate()?;
                run_id.validate()?;
                idempotency_key.validate()?;
                payload.validate()?;
                Ok(())
            }
            Self::Stop {
                command_id,
                run_id,
                idempotency_key,
                payload,
            } => {
                command_id.validate()?;
                run_id.validate()?;
                idempotency_key.validate()?;
                payload.validate()?;
                Ok(())
            }
            Self::AnswerClarification {
                command_id,
                run_id,
                idempotency_key,
                payload,
            } => {
                command_id.validate()?;
                run_id.validate()?;
                idempotency_key.validate()?;
                payload.validate()?;
                Ok(())
            }
            Self::ReviewArtifact {
                command_id,
                run_id,
                idempotency_key,
                payload,
            } => {
                command_id.validate()?;
                run_id.validate()?;
                idempotency_key.validate()?;
                payload.validate()?;
                Ok(())
            }
            Self::DecideAction {
                command_id,
                run_id,
                idempotency_key,
                payload,
            } => {
                command_id.validate()?;
                run_id.validate()?;
                idempotency_key.validate()?;
                payload.validate()?;
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum EventType {
    #[serde(rename = "artifact.committed")]
    ArtifactCommitted,
    #[serde(rename = "attempt.claimed")]
    AttemptClaimed,
    #[serde(rename = "attempt.lost")]
    AttemptLost,
    #[serde(rename = "check.completed")]
    CheckCompleted,
    #[serde(rename = "checkpoint.committed")]
    CheckpointCommitted,
    #[serde(rename = "checkpoint.rejected")]
    CheckpointRejected,
    #[serde(rename = "control.accepted")]
    ControlAccepted,
    #[serde(rename = "control.applied")]
    ControlApplied,
    #[serde(rename = "control.deferred")]
    ControlDeferred,
    #[serde(rename = "control.superseded")]
    ControlSuperseded,
    #[serde(rename = "decision.recorded")]
    DecisionRecorded,
    #[serde(rename = "decision.requested")]
    DecisionRequested,
    #[serde(rename = "handoff.aborted")]
    HandoffAborted,
    #[serde(rename = "handoff.committed")]
    HandoffCommitted,
    #[serde(rename = "handoff.prepared")]
    HandoffPrepared,
    #[serde(rename = "input.accepted")]
    InputAccepted,
    #[serde(rename = "model.completed")]
    ModelCompleted,
    #[serde(rename = "model.interrupted")]
    ModelInterrupted,
    #[serde(rename = "model.requested")]
    ModelRequested,
    #[serde(rename = "observation.accepted")]
    ObservationAccepted,
    #[serde(rename = "process.exited")]
    ProcessExited,
    #[serde(rename = "process.lost")]
    ProcessLost,
    #[serde(rename = "process.output_gap")]
    ProcessOutputGap,
    #[serde(rename = "process.started")]
    ProcessStarted,
    #[serde(rename = "run.accepted")]
    RunAccepted,
    #[serde(rename = "run.cancelled")]
    RunCancelled,
    #[serde(rename = "run.completed")]
    RunCompleted,
    #[serde(rename = "run.failed")]
    RunFailed,
    #[serde(rename = "run.transitioned")]
    RunTransitioned,
    #[serde(rename = "tool.authorized")]
    ToolAuthorized,
    #[serde(rename = "tool.dispatched")]
    ToolDispatched,
    #[serde(rename = "tool.proposed")]
    ToolProposed,
    #[serde(rename = "tool.settled")]
    ToolSettled,
    #[serde(rename = "usage.recorded")]
    UsageRecorded,
    #[serde(rename = "wait.registered")]
    WaitRegistered,
    #[serde(rename = "wait.resolved")]
    WaitResolved,
}
impl Validate for EventType {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub schema_version: ProtocolVersion,
    pub event_id: DomainId,
    pub workspace_id: DomainId,
    pub run_id: DomainId,
    pub attempt_id: Nullable<DomainId>,
    pub turn_id: Nullable<DomainId>,
    pub epoch: Counter,
    pub seq: Counter,
    pub event_type: EventType,
    pub command_id: Nullable<DomainId>,
    pub invocation_id: Nullable<DomainId>,
    pub causal_id: Nullable<DomainId>,
    pub occurred_at: Instant,
    pub recorded_at: Instant,
    pub actor_id: DomainId,
    pub payload_ref: Nullable<DomainId>,
}
impl Validate for Event {
    fn validate(&self) -> Result<(), ContractError> {
        self.schema_version.validate()?;
        self.event_id.validate()?;
        self.workspace_id.validate()?;
        self.run_id.validate()?;
        self.attempt_id.validate()?;
        self.turn_id.validate()?;
        self.epoch.validate()?;
        self.seq.validate()?;
        self.event_type.validate()?;
        self.command_id.validate()?;
        self.invocation_id.validate()?;
        self.causal_id.validate()?;
        self.occurred_at.validate()?;
        self.recorded_at.validate()?;
        self.actor_id.validate()?;
        self.payload_ref.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct ToolResultRunningDataV1Handle {
    pub environment_id: DomainId,
    pub generation: PositiveCounter,
    pub handle: Text<0, 256>,
}
impl Validate for ToolResultRunningDataV1Handle {
    fn validate(&self) -> Result<(), ContractError> {
        self.environment_id.validate()?;
        self.generation.validate()?;
        self.handle.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct ToolResultRunningDataV1 {
    pub handle: ToolResultRunningDataV1Handle,
    pub stdout_cursor: Nullable<Text<0, 2048>>,
    pub stderr_cursor: Nullable<Text<0, 2048>>,
}
impl Validate for ToolResultRunningDataV1 {
    fn validate(&self) -> Result<(), ContractError> {
        self.handle.validate()?;
        self.stdout_cursor.validate()?;
        self.stderr_cursor.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct ToolResultRunningDataV2 {
    pub operation_handle: RemoteOperationHandle,
    pub poll_after_ms: SmallInteger<1000, 120000>,
}
impl Validate for ToolResultRunningDataV2 {
    fn validate(&self) -> Result<(), ContractError> {
        self.operation_handle.validate()?;
        self.poll_after_ms.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(untagged)]
pub enum ToolResultRunningData {
    V1(Box<ToolResultRunningDataV1>),
    V2(Box<ToolResultRunningDataV2>),
}
impl Validate for ToolResultRunningData {
    fn validate(&self) -> Result<(), ContractError> {
        match self {
            Self::V1(v) => v.validate(),
            Self::V2(v) => v.validate(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum ToolResultAwaitingInputDataRequestKind {
    #[serde(rename = "clarification")]
    Clarification,
    #[serde(rename = "action_approval")]
    ActionApproval,
    #[serde(rename = "artifact_review")]
    ArtifactReview,
}
impl Validate for ToolResultAwaitingInputDataRequestKind {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct ToolResultAwaitingInputData {
    pub request_id: DomainId,
    pub wait_id: DomainId,
    pub request_kind: ToolResultAwaitingInputDataRequestKind,
}
impl Validate for ToolResultAwaitingInputData {
    fn validate(&self) -> Result<(), ContractError> {
        self.request_id.validate()?;
        self.wait_id.validate()?;
        self.request_kind.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(tag = "status")]
#[serde(deny_unknown_fields)]
pub enum ToolResult {
    #[serde(rename = "succeeded")]
    Succeeded {
        invocation_id: DomainId,
        schema_version: ProtocolVersion,
        artifact_refs: List<ArtifactRef, 0, 100>,
        receipt_ref: Nullable<DomainId>,
        truncated: bool,
        next_cursor: Nullable<Text<0, 2048>>,
        usage_refs: List<DomainId, 0, 100>,
        data: BoundData,
    },
    #[serde(rename = "running")]
    Running {
        invocation_id: DomainId,
        schema_version: ProtocolVersion,
        artifact_refs: List<ArtifactRef, 0, 100>,
        receipt_ref: Nullable<DomainId>,
        truncated: bool,
        next_cursor: Nullable<Text<0, 2048>>,
        usage_refs: List<DomainId, 0, 100>,
        data: ToolResultRunningData,
    },
    #[serde(rename = "awaiting_input")]
    AwaitingInput {
        invocation_id: DomainId,
        schema_version: ProtocolVersion,
        artifact_refs: List<ArtifactRef, 0, 100>,
        receipt_ref: Nullable<DomainId>,
        truncated: bool,
        next_cursor: Nullable<Text<0, 2048>>,
        usage_refs: List<DomainId, 0, 100>,
        data: ToolResultAwaitingInputData,
    },
    #[serde(rename = "denied")]
    Denied {
        invocation_id: DomainId,
        schema_version: ProtocolVersion,
        artifact_refs: List<ArtifactRef, 0, 100>,
        receipt_ref: Nullable<DomainId>,
        truncated: bool,
        next_cursor: Nullable<Text<0, 2048>>,
        usage_refs: List<DomainId, 0, 100>,
        error: SafeError,
        data: Nullable<BoundData>,
    },
    #[serde(rename = "conflict")]
    Conflict {
        invocation_id: DomainId,
        schema_version: ProtocolVersion,
        artifact_refs: List<ArtifactRef, 0, 100>,
        receipt_ref: Nullable<DomainId>,
        truncated: bool,
        next_cursor: Nullable<Text<0, 2048>>,
        usage_refs: List<DomainId, 0, 100>,
        error: SafeError,
        data: Nullable<BoundData>,
    },
    #[serde(rename = "timed_out")]
    TimedOut {
        invocation_id: DomainId,
        schema_version: ProtocolVersion,
        artifact_refs: List<ArtifactRef, 0, 100>,
        receipt_ref: Nullable<DomainId>,
        truncated: bool,
        next_cursor: Nullable<Text<0, 2048>>,
        usage_refs: List<DomainId, 0, 100>,
        error: SafeError,
        data: Nullable<BoundData>,
    },
    #[serde(rename = "cancelled")]
    Cancelled {
        invocation_id: DomainId,
        schema_version: ProtocolVersion,
        artifact_refs: List<ArtifactRef, 0, 100>,
        receipt_ref: Nullable<DomainId>,
        truncated: bool,
        next_cursor: Nullable<Text<0, 2048>>,
        usage_refs: List<DomainId, 0, 100>,
        error: SafeError,
        data: Nullable<BoundData>,
    },
    #[serde(rename = "lost")]
    Lost {
        invocation_id: DomainId,
        schema_version: ProtocolVersion,
        artifact_refs: List<ArtifactRef, 0, 100>,
        receipt_ref: Nullable<DomainId>,
        truncated: bool,
        next_cursor: Nullable<Text<0, 2048>>,
        usage_refs: List<DomainId, 0, 100>,
        error: SafeError,
        data: Nullable<BoundData>,
    },
    #[serde(rename = "unknown")]
    Unknown {
        invocation_id: DomainId,
        schema_version: ProtocolVersion,
        artifact_refs: List<ArtifactRef, 0, 100>,
        receipt_ref: Nullable<DomainId>,
        truncated: bool,
        next_cursor: Nullable<Text<0, 2048>>,
        usage_refs: List<DomainId, 0, 100>,
        error: SafeError,
        data: Nullable<BoundData>,
    },
    #[serde(rename = "failed")]
    Failed {
        invocation_id: DomainId,
        schema_version: ProtocolVersion,
        artifact_refs: List<ArtifactRef, 0, 100>,
        receipt_ref: Nullable<DomainId>,
        truncated: bool,
        next_cursor: Nullable<Text<0, 2048>>,
        usage_refs: List<DomainId, 0, 100>,
        error: SafeError,
        data: Nullable<BoundData>,
    },
}
impl Validate for ToolResult {
    fn validate(&self) -> Result<(), ContractError> {
        match self {
            Self::Succeeded {
                invocation_id,
                schema_version,
                artifact_refs,
                receipt_ref,
                truncated,
                next_cursor,
                usage_refs,
                data,
            } => {
                invocation_id.validate()?;
                schema_version.validate()?;
                artifact_refs.validate()?;
                receipt_ref.validate()?;
                truncated.validate()?;
                next_cursor.validate()?;
                usage_refs.validate()?;
                data.validate()?;
                Ok(())
            }
            Self::Running {
                invocation_id,
                schema_version,
                artifact_refs,
                receipt_ref,
                truncated,
                next_cursor,
                usage_refs,
                data,
            } => {
                invocation_id.validate()?;
                schema_version.validate()?;
                artifact_refs.validate()?;
                receipt_ref.validate()?;
                truncated.validate()?;
                next_cursor.validate()?;
                usage_refs.validate()?;
                data.validate()?;
                Ok(())
            }
            Self::AwaitingInput {
                invocation_id,
                schema_version,
                artifact_refs,
                receipt_ref,
                truncated,
                next_cursor,
                usage_refs,
                data,
            } => {
                invocation_id.validate()?;
                schema_version.validate()?;
                artifact_refs.validate()?;
                receipt_ref.validate()?;
                truncated.validate()?;
                next_cursor.validate()?;
                usage_refs.validate()?;
                data.validate()?;
                Ok(())
            }
            Self::Denied {
                invocation_id,
                schema_version,
                artifact_refs,
                receipt_ref,
                truncated,
                next_cursor,
                usage_refs,
                error,
                data,
            } => {
                invocation_id.validate()?;
                schema_version.validate()?;
                artifact_refs.validate()?;
                receipt_ref.validate()?;
                truncated.validate()?;
                next_cursor.validate()?;
                usage_refs.validate()?;
                error.validate()?;
                data.validate()?;
                Ok(())
            }
            Self::Conflict {
                invocation_id,
                schema_version,
                artifact_refs,
                receipt_ref,
                truncated,
                next_cursor,
                usage_refs,
                error,
                data,
            } => {
                invocation_id.validate()?;
                schema_version.validate()?;
                artifact_refs.validate()?;
                receipt_ref.validate()?;
                truncated.validate()?;
                next_cursor.validate()?;
                usage_refs.validate()?;
                error.validate()?;
                data.validate()?;
                Ok(())
            }
            Self::TimedOut {
                invocation_id,
                schema_version,
                artifact_refs,
                receipt_ref,
                truncated,
                next_cursor,
                usage_refs,
                error,
                data,
            } => {
                invocation_id.validate()?;
                schema_version.validate()?;
                artifact_refs.validate()?;
                receipt_ref.validate()?;
                truncated.validate()?;
                next_cursor.validate()?;
                usage_refs.validate()?;
                error.validate()?;
                data.validate()?;
                Ok(())
            }
            Self::Cancelled {
                invocation_id,
                schema_version,
                artifact_refs,
                receipt_ref,
                truncated,
                next_cursor,
                usage_refs,
                error,
                data,
            } => {
                invocation_id.validate()?;
                schema_version.validate()?;
                artifact_refs.validate()?;
                receipt_ref.validate()?;
                truncated.validate()?;
                next_cursor.validate()?;
                usage_refs.validate()?;
                error.validate()?;
                data.validate()?;
                Ok(())
            }
            Self::Lost {
                invocation_id,
                schema_version,
                artifact_refs,
                receipt_ref,
                truncated,
                next_cursor,
                usage_refs,
                error,
                data,
            } => {
                invocation_id.validate()?;
                schema_version.validate()?;
                artifact_refs.validate()?;
                receipt_ref.validate()?;
                truncated.validate()?;
                next_cursor.validate()?;
                usage_refs.validate()?;
                error.validate()?;
                data.validate()?;
                Ok(())
            }
            Self::Unknown {
                invocation_id,
                schema_version,
                artifact_refs,
                receipt_ref,
                truncated,
                next_cursor,
                usage_refs,
                error,
                data,
            } => {
                invocation_id.validate()?;
                schema_version.validate()?;
                artifact_refs.validate()?;
                receipt_ref.validate()?;
                truncated.validate()?;
                next_cursor.validate()?;
                usage_refs.validate()?;
                error.validate()?;
                data.validate()?;
                Ok(())
            }
            Self::Failed {
                invocation_id,
                schema_version,
                artifact_refs,
                receipt_ref,
                truncated,
                next_cursor,
                usage_refs,
                error,
                data,
            } => {
                invocation_id.validate()?;
                schema_version.validate()?;
                artifact_refs.validate()?;
                receipt_ref.validate()?;
                truncated.validate()?;
                next_cursor.validate()?;
                usage_refs.validate()?;
                error.validate()?;
                data.validate()?;
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct BoundData {
    pub binding_id: DomainId,
    #[cfg_attr(feature = "typescript-export", ts(type = "unknown"))]
    pub value: serde_json::Value,
}
impl Validate for BoundData {
    fn validate(&self) -> Result<(), ContractError> {
        self.binding_id.validate()?;
        self.value.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum WaitSpecMode {
    #[serde(rename = "all")]
    All,
    #[serde(rename = "any")]
    Any,
}
impl Validate for WaitSpecMode {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct WaitSpecConditionsItemProcessHandle {
    pub environment_id: DomainId,
    pub generation: PositiveCounter,
    pub handle: Text<0, 256>,
}
impl Validate for WaitSpecConditionsItemProcessHandle {
    fn validate(&self) -> Result<(), ContractError> {
        self.environment_id.validate()?;
        self.generation.validate()?;
        self.handle.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(tag = "kind")]
#[serde(deny_unknown_fields)]
pub enum WaitSpecConditionsItem {
    #[serde(rename = "human")]
    Human {
        request_id: DomainId,
        request_revision: PositiveCounter,
    },
    #[serde(rename = "timer")]
    Timer { due_at: Instant },
    #[serde(rename = "event")]
    Event {
        resource_id: DomainId,
        expected_version_id: DomainId,
        event_kind: Text<0, 128>,
        after_watermark: Counter,
    },
    #[serde(rename = "process")]
    Process {
        handle: WaitSpecConditionsItemProcessHandle,
    },
    #[serde(rename = "child")]
    Child {
        child_run_id: DomainId,
        required_output_schema_digest: Digest,
    },
    #[serde(rename = "budget")]
    Budget { budget_period_id: DomainId },
}
impl Validate for WaitSpecConditionsItem {
    fn validate(&self) -> Result<(), ContractError> {
        match self {
            Self::Human {
                request_id,
                request_revision,
            } => {
                request_id.validate()?;
                request_revision.validate()?;
                Ok(())
            }
            Self::Timer { due_at } => {
                due_at.validate()?;
                Ok(())
            }
            Self::Event {
                resource_id,
                expected_version_id,
                event_kind,
                after_watermark,
            } => {
                resource_id.validate()?;
                expected_version_id.validate()?;
                event_kind.validate()?;
                after_watermark.validate()?;
                Ok(())
            }
            Self::Process { handle } => {
                handle.validate()?;
                Ok(())
            }
            Self::Child {
                child_run_id,
                required_output_schema_digest,
            } => {
                child_run_id.validate()?;
                required_output_schema_digest.validate()?;
                Ok(())
            }
            Self::Budget { budget_period_id } => {
                budget_period_id.validate()?;
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct WaitSpec {
    pub wait_id: DomainId,
    pub mode: WaitSpecMode,
    pub conditions: List<WaitSpecConditionsItem, 1, 20>,
    pub watermark: Counter,
    pub deadline: Nullable<Instant>,
}
impl Validate for WaitSpec {
    fn validate(&self) -> Result<(), ContractError> {
        self.wait_id.validate()?;
        self.mode.validate()?;
        self.conditions.validate()?;
        self.watermark.validate()?;
        self.deadline.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum FileCoverage {
    #[serde(rename = "coherent_snapshot")]
    CoherentSnapshot,
    #[serde(rename = "verified_per_file")]
    VerifiedPerFile,
    #[serde(rename = "no_files")]
    NoFiles,
}
impl Validate for FileCoverage {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub schema_version: ProtocolVersion,
    pub checkpoint_id: DomainId,
    pub run_id: DomainId,
    pub attempt_id: DomainId,
    pub epoch: PositiveCounter,
    pub event_seq: Counter,
    pub accepted_seq: Counter,
    pub applied_seq: Counter,
    pub task_revision_id: DomainId,
    pub session_id: DomainId,
    pub branch_id: DomainId,
    pub plan_version_id: Nullable<DomainId>,
    pub graph_version_id: Nullable<DomainId>,
    pub pending_decision_ids: List<DomainId, 0, 100>,
    pub unresolved_invocation_ids: List<DomainId, 0, 100>,
    pub committed_model_result_refs: List<DomainId, 0, 1000>,
    pub source_memory_version_refs: List<DomainId, 0, 1000>,
    pub manifest_id: DomainId,
    pub manifest_digest: Digest,
    pub file_coverage: FileCoverage,
    pub file_gap_description: Nullable<Text<0, 2048>>,
    pub process_disposition_refs: List<DomainId, 0, 100>,
    pub environment_recipe_digest: Nullable<Digest>,
    pub permission_generation: PositiveCounter,
    pub deletion_generation: Counter,
    pub usage_refs: List<DomainId, 0, 100>,
    pub next_continuation_ref: DomainId,
}
impl Validate for Checkpoint {
    fn validate(&self) -> Result<(), ContractError> {
        self.schema_version.validate()?;
        self.checkpoint_id.validate()?;
        self.run_id.validate()?;
        self.attempt_id.validate()?;
        self.epoch.validate()?;
        self.event_seq.validate()?;
        self.accepted_seq.validate()?;
        self.applied_seq.validate()?;
        self.task_revision_id.validate()?;
        self.session_id.validate()?;
        self.branch_id.validate()?;
        self.plan_version_id.validate()?;
        self.graph_version_id.validate()?;
        self.pending_decision_ids.validate()?;
        self.unresolved_invocation_ids.validate()?;
        self.committed_model_result_refs.validate()?;
        self.source_memory_version_refs.validate()?;
        self.manifest_id.validate()?;
        self.manifest_digest.validate()?;
        self.file_coverage.validate()?;
        self.file_gap_description.validate()?;
        self.process_disposition_refs.validate()?;
        self.environment_recipe_digest.validate()?;
        self.permission_generation.validate()?;
        self.deletion_generation.validate()?;
        self.usage_refs.validate()?;
        self.next_continuation_ref.validate()?;
        crate::semantics::checkpoint(self)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum TurnOutcomeYieldedReason {
    #[serde(rename = "turn_limit")]
    TurnLimit,
    #[serde(rename = "context_limit")]
    ContextLimit,
    #[serde(rename = "user")]
    User,
    #[serde(rename = "budget")]
    Budget,
    #[serde(rename = "resource_limit")]
    ResourceLimit,
}
impl Validate for TurnOutcomeYieldedReason {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum TurnOutcomeFailedRetryClass {
    #[serde(rename = "safe_read")]
    SafeRead,
    #[serde(rename = "reconcile_first")]
    ReconcileFirst,
    #[serde(rename = "remediation")]
    Remediation,
    #[serde(rename = "terminal")]
    Terminal,
}
impl Validate for TurnOutcomeFailedRetryClass {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(tag = "kind")]
#[serde(deny_unknown_fields)]
pub enum TurnOutcome {
    #[serde(rename = "candidate_result")]
    CandidateResult {
        objective_version_id: DomainId,
        artifact_refs: List<ArtifactRef, 0, 100>,
        check_result_refs: List<DomainId, 0, 100>,
        checkpoint_id: DomainId,
    },
    #[serde(rename = "wait_requested")]
    WaitRequested {
        wait: WaitSpec,
        checkpoint_id: DomainId,
    },
    #[serde(rename = "yielded")]
    Yielded {
        reason: TurnOutcomeYieldedReason,
        checkpoint_id: DomainId,
    },
    #[serde(rename = "paused")]
    Paused {
        reason: TurnOutcomeYieldedReason,
        checkpoint_id: DomainId,
    },
    #[serde(rename = "cancelled")]
    Cancelled {
        checkpoint_id: Nullable<DomainId>,
        cleanup_refs: List<DomainId, 0, 100>,
        unresolved_receipt_refs: List<DomainId, 0, 100>,
    },
    #[serde(rename = "failed")]
    Failed {
        error: SafeError,
        retry_class: TurnOutcomeFailedRetryClass,
        checkpoint_id: Nullable<DomainId>,
    },
}
impl Validate for TurnOutcome {
    fn validate(&self) -> Result<(), ContractError> {
        match self {
            Self::CandidateResult {
                objective_version_id,
                artifact_refs,
                check_result_refs,
                checkpoint_id,
            } => {
                objective_version_id.validate()?;
                artifact_refs.validate()?;
                check_result_refs.validate()?;
                checkpoint_id.validate()?;
                Ok(())
            }
            Self::WaitRequested {
                wait,
                checkpoint_id,
            } => {
                wait.validate()?;
                checkpoint_id.validate()?;
                Ok(())
            }
            Self::Yielded {
                reason,
                checkpoint_id,
            } => {
                reason.validate()?;
                checkpoint_id.validate()?;
                Ok(())
            }
            Self::Paused {
                reason,
                checkpoint_id,
            } => {
                reason.validate()?;
                checkpoint_id.validate()?;
                Ok(())
            }
            Self::Cancelled {
                checkpoint_id,
                cleanup_refs,
                unresolved_receipt_refs,
            } => {
                checkpoint_id.validate()?;
                cleanup_refs.validate()?;
                unresolved_receipt_refs.validate()?;
                Ok(())
            }
            Self::Failed {
                error,
                retry_class,
                checkpoint_id,
            } => {
                error.validate()?;
                retry_class.validate()?;
                checkpoint_id.validate()?;
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RemoteOperationHandle {
    pub installation_id: DomainId,
    pub invocation_id: DomainId,
    pub opaque_handle: Text<0, 2048>,
    pub expires_at: Instant,
}
impl Validate for RemoteOperationHandle {
    fn validate(&self) -> Result<(), ContractError> {
        self.installation_id.validate()?;
        self.invocation_id.validate()?;
        self.opaque_handle.validate()?;
        self.expires_at.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub model_steps: SmallInteger<1, 50>,
    pub active_wall_ms: SmallInteger<1, 600000>,
    pub output_bytes: SmallInteger<1, 262144>,
}
impl Validate for Limits {
    fn validate(&self) -> Result<(), ContractError> {
        self.model_steps.validate()?;
        self.active_wall_ms.validate()?;
        self.output_bytes.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct CapabilityDescriptionProfilesItem {
    pub id: Text<0, 128>,
    pub enabled: bool,
    pub reason: Nullable<Text<0, 1024>>,
}
impl Validate for CapabilityDescriptionProfilesItem {
    fn validate(&self) -> Result<(), ContractError> {
        self.id.validate()?;
        self.enabled.validate()?;
        self.reason.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct CapabilityDescription {
    pub protocol_version: ProtocolVersion,
    pub build_digest: Digest,
    pub schema_versions: List<Text<0, 128>, 0, 100>,
    pub operation_names: List<Text<0, 128>, 0, 100>,
    pub profiles: List<CapabilityDescriptionProfilesItem, 0, 100>,
    pub capability_manifest_digest: Digest,
}
impl Validate for CapabilityDescription {
    fn validate(&self) -> Result<(), ContractError> {
        self.protocol_version.validate()?;
        self.build_digest.validate()?;
        self.schema_versions.validate()?;
        self.operation_names.validate()?;
        self.profiles.validate()?;
        self.capability_manifest_digest.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum RunState {
    #[serde(rename = "queued")]
    Queued,
    #[serde(rename = "preparing")]
    Preparing,
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "waiting")]
    Waiting,
    #[serde(rename = "paused")]
    Paused,
    #[serde(rename = "blocked")]
    Blocked,
    #[serde(rename = "transferring")]
    Transferring,
    #[serde(rename = "verifying")]
    Verifying,
    #[serde(rename = "recovering")]
    Recovering,
    #[serde(rename = "cancelling")]
    Cancelling,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "cancelled")]
    Cancelled,
    #[serde(rename = "failed")]
    Failed,
}
impl Validate for RunState {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct TaskSnapshot {
    pub task_id: DomainId,
    pub task_revision_id: DomainId,
    pub run_id: DomainId,
    pub row_version: PositiveCounter,
    pub state: RunState,
    pub authority: Authority,
    pub placement: Placement,
    pub accepted_seq: Counter,
    pub applied_seq: Counter,
    pub last_event_seq: Counter,
    pub cancellation_requested: bool,
    pub artifact_refs: List<ArtifactRef, 0, 100>,
    pub check_refs: List<DomainId, 0, 100>,
    pub receipt_refs: List<DomainId, 0, 100>,
    pub wait_refs: List<DomainId, 0, 100>,
    pub coverage_manifest_ref: Nullable<DomainId>,
}
impl Validate for TaskSnapshot {
    fn validate(&self) -> Result<(), ContractError> {
        self.task_id.validate()?;
        self.task_revision_id.validate()?;
        self.run_id.validate()?;
        self.row_version.validate()?;
        self.state.validate()?;
        self.authority.validate()?;
        self.placement.validate()?;
        self.accepted_seq.validate()?;
        self.applied_seq.validate()?;
        self.last_event_seq.validate()?;
        self.cancellation_requested.validate()?;
        self.artifact_refs.validate()?;
        self.check_refs.validate()?;
        self.receipt_refs.validate()?;
        self.wait_refs.validate()?;
        self.coverage_manifest_ref.validate()?;
        crate::semantics::snapshot(self)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum TransferDisposition {
    #[serde(rename = "preflight")]
    Preflight,
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "committed")]
    Committed,
    #[serde(rename = "aborted")]
    Aborted,
    #[serde(rename = "blocked")]
    Blocked,
}
impl Validate for TransferDisposition {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct TransferReceipt {
    pub transfer_id: DomainId,
    pub run_id: DomainId,
    pub disposition: TransferDisposition,
    pub authority: Authority,
    pub source_epoch: PositiveCounter,
    pub destination_epoch: Nullable<PositiveCounter>,
    pub accepted_seq: Counter,
    pub receipt_ref: Nullable<DomainId>,
    pub gap_codes: List<Text<0, 128>, 0, 100>,
}
impl Validate for TransferReceipt {
    fn validate(&self) -> Result<(), ContractError> {
        self.transfer_id.validate()?;
        self.run_id.validate()?;
        self.disposition.validate()?;
        self.authority.validate()?;
        self.source_epoch.validate()?;
        self.destination_epoch.validate()?;
        self.accepted_seq.validate()?;
        self.receipt_ref.validate()?;
        self.gap_codes.validate()?;
        crate::semantics::transfer(self)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct Subscription {
    pub subscription_id: DomainId,
    pub snapshot: TaskSnapshot,
    pub replay_after: Counter,
    pub resync_required: bool,
}
impl Validate for Subscription {
    fn validate(&self) -> Result<(), ContractError> {
        self.subscription_id.validate()?;
        self.snapshot.validate()?;
        self.replay_after.validate()?;
        self.resync_required.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct ArtifactReadResult {
    pub artifact: ArtifactRef,
    pub media_type: Text<0, 128>,
    pub content_handle: Nullable<DomainId>,
    pub next_cursor: Nullable<Text<0, 2048>>,
    pub truncated: bool,
}
impl Validate for ArtifactReadResult {
    fn validate(&self) -> Result<(), ContractError> {
        self.artifact.validate()?;
        self.media_type.validate()?;
        self.content_handle.validate()?;
        self.next_cursor.validate()?;
        self.truncated.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum UsageReportCurrency {
    #[serde(rename = "USD")]
    Usd,
}
impl Validate for UsageReportCurrency {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum UsageReportSegmentsItemKind {
    #[serde(rename = "local")]
    Local,
    #[serde(rename = "cloud_agent")]
    CloudAgent,
    #[serde(rename = "worker")]
    Worker,
}
impl Validate for UsageReportSegmentsItemKind {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct UsageReportSegmentsItem {
    pub kind: UsageReportSegmentsItemKind,
    pub settled_amount: Money,
    pub estimated_amount: Nullable<Money>,
    pub unsettled_refs: List<DomainId, 0, 100>,
    pub budget_period_refs: List<DomainId, 0, 100>,
}
impl Validate for UsageReportSegmentsItem {
    fn validate(&self) -> Result<(), ContractError> {
        self.kind.validate()?;
        self.settled_amount.validate()?;
        self.estimated_amount.validate()?;
        self.unsettled_refs.validate()?;
        self.budget_period_refs.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct UsageReport {
    pub scope_id: DomainId,
    pub currency: UsageReportCurrency,
    pub segments: List<UsageReportSegmentsItem, 0, 100>,
}
impl Validate for UsageReport {
    fn validate(&self) -> Result<(), ContractError> {
        self.scope_id.validate()?;
        self.currency.validate()?;
        self.segments.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum RequestFrameKind {
    #[serde(rename = "request")]
    Request,
}
impl Validate for RequestFrameKind {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum RpcRequestHandshakeBodyClientKind {
    #[serde(rename = "electron_main")]
    ElectronMain,
    #[serde(rename = "supervisor")]
    Supervisor,
    #[serde(rename = "harness")]
    Harness,
    #[serde(rename = "guest_agent")]
    GuestAgent,
}
impl Validate for RpcRequestHandshakeBodyClientKind {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcRequestHandshakeBody {
    pub supported_versions: List<Text<0, 128>, 0, 10>,
    pub client_kind: RpcRequestHandshakeBodyClientKind,
    pub build_digest: Digest,
    pub nonce: Text<0, 256>,
    pub challenge_response: Text<0, 512>,
    pub requested_workspace_id: Nullable<DomainId>,
}
impl Validate for RpcRequestHandshakeBody {
    fn validate(&self) -> Result<(), ContractError> {
        self.supported_versions.validate()?;
        self.client_kind.validate()?;
        self.build_digest.validate()?;
        self.nonce.validate()?;
        self.challenge_response.validate()?;
        self.requested_workspace_id.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcRequestDescribeCapabilitiesBody {}
impl Validate for RpcRequestDescribeCapabilitiesBody {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcRequestSubmitTaskBody {
    pub objective: Text<0, 8192>,
    pub project_id: Nullable<DomainId>,
    pub source_refs: List<DomainId, 0, 100>,
    pub check_template_ids: List<DomainId, 0, 20>,
    pub requested_placement: Placement,
    pub profile_id: Text<0, 128>,
    pub lower_limits: Limits,
    pub idempotency_key: Text<0, 128>,
}
impl Validate for RpcRequestSubmitTaskBody {
    fn validate(&self) -> Result<(), ContractError> {
        self.objective.validate()?;
        self.project_id.validate()?;
        self.source_refs.validate()?;
        self.check_template_ids.validate()?;
        self.requested_placement.validate()?;
        self.profile_id.validate()?;
        self.lower_limits.validate()?;
        self.idempotency_key.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcRequestSubmitCommandBodySteerPayload {
    pub text: Text<0, 8192>,
    pub expected_revision: PositiveCounter,
}
impl Validate for RpcRequestSubmitCommandBodySteerPayload {
    fn validate(&self) -> Result<(), ContractError> {
        self.text.validate()?;
        self.expected_revision.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcRequestSubmitCommandBodyReviseObjectivePayload {
    pub objective_version_id: DomainId,
    pub check_refs: List<DomainId, 0, 100>,
    pub expected_revision: PositiveCounter,
}
impl Validate for RpcRequestSubmitCommandBodyReviseObjectivePayload {
    fn validate(&self) -> Result<(), ContractError> {
        self.objective_version_id.validate()?;
        self.check_refs.validate()?;
        self.expected_revision.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcRequestSubmitCommandBodyPausePayload {
    pub expected_revision: PositiveCounter,
    pub reason: Text<0, 1024>,
}
impl Validate for RpcRequestSubmitCommandBodyPausePayload {
    fn validate(&self) -> Result<(), ContractError> {
        self.expected_revision.validate()?;
        self.reason.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcRequestSubmitCommandBodyResumePayload {
    pub expected_revision: PositiveCounter,
    pub checkpoint_id: DomainId,
}
impl Validate for RpcRequestSubmitCommandBodyResumePayload {
    fn validate(&self) -> Result<(), ContractError> {
        self.expected_revision.validate()?;
        self.checkpoint_id.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcRequestSubmitCommandBodyStopPayload {
    pub reason: Text<0, 1024>,
}
impl Validate for RpcRequestSubmitCommandBodyStopPayload {
    fn validate(&self) -> Result<(), ContractError> {
        self.reason.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(tag = "kind")]
#[serde(deny_unknown_fields)]
pub enum RpcRequestSubmitCommandBody {
    #[serde(rename = "steer")]
    Steer {
        command_id: DomainId,
        run_id: DomainId,
        idempotency_key: Text<1, 128>,
        payload: RpcRequestSubmitCommandBodySteerPayload,
    },
    #[serde(rename = "revise_objective")]
    ReviseObjective {
        command_id: DomainId,
        run_id: DomainId,
        idempotency_key: Text<1, 128>,
        payload: RpcRequestSubmitCommandBodyReviseObjectivePayload,
    },
    #[serde(rename = "pause")]
    Pause {
        command_id: DomainId,
        run_id: DomainId,
        idempotency_key: Text<1, 128>,
        payload: RpcRequestSubmitCommandBodyPausePayload,
    },
    #[serde(rename = "resume")]
    Resume {
        command_id: DomainId,
        run_id: DomainId,
        idempotency_key: Text<1, 128>,
        payload: RpcRequestSubmitCommandBodyResumePayload,
    },
    #[serde(rename = "stop")]
    Stop {
        command_id: DomainId,
        run_id: DomainId,
        idempotency_key: Text<1, 128>,
        payload: RpcRequestSubmitCommandBodyStopPayload,
    },
}
impl Validate for RpcRequestSubmitCommandBody {
    fn validate(&self) -> Result<(), ContractError> {
        match self {
            Self::Steer {
                command_id,
                run_id,
                idempotency_key,
                payload,
            } => {
                command_id.validate()?;
                run_id.validate()?;
                idempotency_key.validate()?;
                payload.validate()?;
                Ok(())
            }
            Self::ReviseObjective {
                command_id,
                run_id,
                idempotency_key,
                payload,
            } => {
                command_id.validate()?;
                run_id.validate()?;
                idempotency_key.validate()?;
                payload.validate()?;
                Ok(())
            }
            Self::Pause {
                command_id,
                run_id,
                idempotency_key,
                payload,
            } => {
                command_id.validate()?;
                run_id.validate()?;
                idempotency_key.validate()?;
                payload.validate()?;
                Ok(())
            }
            Self::Resume {
                command_id,
                run_id,
                idempotency_key,
                payload,
            } => {
                command_id.validate()?;
                run_id.validate()?;
                idempotency_key.validate()?;
                payload.validate()?;
                Ok(())
            }
            Self::Stop {
                command_id,
                run_id,
                idempotency_key,
                payload,
            } => {
                command_id.validate()?;
                run_id.validate()?;
                idempotency_key.validate()?;
                payload.validate()?;
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcRequestGetTaskSnapshotBody {
    pub run_id: DomainId,
}
impl Validate for RpcRequestGetTaskSnapshotBody {
    fn validate(&self) -> Result<(), ContractError> {
        self.run_id.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcRequestSubscribeBody {
    pub run_id: DomainId,
    pub after_sequence: Counter,
    pub provisional_cursor: Nullable<Text<0, 2048>>,
}
impl Validate for RpcRequestSubscribeBody {
    fn validate(&self) -> Result<(), ContractError> {
        self.run_id.validate()?;
        self.after_sequence.validate()?;
        self.provisional_cursor.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum RpcRequestAnswerClarificationBodyKind {
    #[serde(rename = "answer_clarification")]
    AnswerClarification,
}
impl Validate for RpcRequestAnswerClarificationBodyKind {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcRequestAnswerClarificationBodyPayloadAnswersItem {
    pub question_id: Text<0, 128>,
    pub answer: Text<0, 8192>,
}
impl Validate for RpcRequestAnswerClarificationBodyPayloadAnswersItem {
    fn validate(&self) -> Result<(), ContractError> {
        self.question_id.validate()?;
        self.answer.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcRequestAnswerClarificationBodyPayload {
    pub request_id: DomainId,
    pub request_revision: PositiveCounter,
    pub answers: List<RpcRequestAnswerClarificationBodyPayloadAnswersItem, 0, 3>,
}
impl Validate for RpcRequestAnswerClarificationBodyPayload {
    fn validate(&self) -> Result<(), ContractError> {
        self.request_id.validate()?;
        self.request_revision.validate()?;
        self.answers.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcRequestAnswerClarificationBody {
    pub command_id: DomainId,
    pub run_id: DomainId,
    pub idempotency_key: Text<1, 128>,
    pub kind: RpcRequestAnswerClarificationBodyKind,
    pub payload: RpcRequestAnswerClarificationBodyPayload,
}
impl Validate for RpcRequestAnswerClarificationBody {
    fn validate(&self) -> Result<(), ContractError> {
        self.command_id.validate()?;
        self.run_id.validate()?;
        self.idempotency_key.validate()?;
        self.kind.validate()?;
        self.payload.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum RpcRequestReviewArtifactBodyKind {
    #[serde(rename = "review_artifact")]
    ReviewArtifact,
}
impl Validate for RpcRequestReviewArtifactBodyKind {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcRequestReviewArtifactBodyPayload {
    pub request_id: DomainId,
    pub request_revision: PositiveCounter,
    pub artifact_id: DomainId,
    pub version_id: DomainId,
    pub check_refs: List<DomainId, 0, 100>,
    pub response: CommandReviewArtifactPayloadResponse,
}
impl Validate for RpcRequestReviewArtifactBodyPayload {
    fn validate(&self) -> Result<(), ContractError> {
        self.request_id.validate()?;
        self.request_revision.validate()?;
        self.artifact_id.validate()?;
        self.version_id.validate()?;
        self.check_refs.validate()?;
        self.response.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcRequestReviewArtifactBody {
    pub command_id: DomainId,
    pub run_id: DomainId,
    pub idempotency_key: Text<1, 128>,
    pub kind: RpcRequestReviewArtifactBodyKind,
    pub payload: RpcRequestReviewArtifactBodyPayload,
}
impl Validate for RpcRequestReviewArtifactBody {
    fn validate(&self) -> Result<(), ContractError> {
        self.command_id.validate()?;
        self.run_id.validate()?;
        self.idempotency_key.validate()?;
        self.kind.validate()?;
        self.payload.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum RpcRequestDecideActionBodyKind {
    #[serde(rename = "decide_action")]
    DecideAction,
}
impl Validate for RpcRequestDecideActionBodyKind {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcRequestDecideActionBodyPayload {
    pub decision_id: DomainId,
    pub decision_revision: PositiveCounter,
    pub invocation_id: DomainId,
    pub target_digest: Digest,
    pub payload_digest: Digest,
    pub response: CommandDecideActionPayloadResponse,
}
impl Validate for RpcRequestDecideActionBodyPayload {
    fn validate(&self) -> Result<(), ContractError> {
        self.decision_id.validate()?;
        self.decision_revision.validate()?;
        self.invocation_id.validate()?;
        self.target_digest.validate()?;
        self.payload_digest.validate()?;
        self.response.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcRequestDecideActionBody {
    pub command_id: DomainId,
    pub run_id: DomainId,
    pub idempotency_key: Text<1, 128>,
    pub kind: RpcRequestDecideActionBodyKind,
    pub payload: RpcRequestDecideActionBodyPayload,
}
impl Validate for RpcRequestDecideActionBody {
    fn validate(&self) -> Result<(), ContractError> {
        self.command_id.validate()?;
        self.run_id.validate()?;
        self.idempotency_key.validate()?;
        self.kind.validate()?;
        self.payload.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcRequestGetArtifactBody {
    pub artifact_id: DomainId,
    pub version_id: DomainId,
    pub cursor: Nullable<Text<0, 2048>>,
    pub max_bytes: SmallInteger<1, 262144>,
}
impl Validate for RpcRequestGetArtifactBody {
    fn validate(&self) -> Result<(), ContractError> {
        self.artifact_id.validate()?;
        self.version_id.validate()?;
        self.cursor.validate()?;
        self.max_bytes.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcRequestPrepareTransferBody {
    pub run_id: DomainId,
    pub expected_revision: PositiveCounter,
    pub target_profile_id: Text<0, 128>,
    pub selected_manifest_id: DomainId,
    pub idempotency_key: Text<0, 128>,
}
impl Validate for RpcRequestPrepareTransferBody {
    fn validate(&self) -> Result<(), ContractError> {
        self.run_id.validate()?;
        self.expected_revision.validate()?;
        self.target_profile_id.validate()?;
        self.selected_manifest_id.validate()?;
        self.idempotency_key.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcRequestQueryTransferBody {
    pub transfer_id: DomainId,
}
impl Validate for RpcRequestQueryTransferBody {
    fn validate(&self) -> Result<(), ContractError> {
        self.transfer_id.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcRequestRequestTransferAbortBody {
    pub transfer_id: DomainId,
    pub idempotency_key: Text<0, 128>,
}
impl Validate for RpcRequestRequestTransferAbortBody {
    fn validate(&self) -> Result<(), ContractError> {
        self.transfer_id.validate()?;
        self.idempotency_key.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcRequestGetUsageBody {
    pub scope_id: DomainId,
    pub starts_at: Instant,
    pub ends_at: Instant,
}
impl Validate for RpcRequestGetUsageBody {
    fn validate(&self) -> Result<(), ContractError> {
        self.scope_id.validate()?;
        self.starts_at.validate()?;
        self.ends_at.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcRequestHarnessDescribeCapabilitiesBody {}
impl Validate for RpcRequestHarnessDescribeCapabilitiesBody {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcRequestHarnessStartTurnBody {
    pub run_spec: RunSpec,
    pub lease: Lease,
    pub turn_id: DomainId,
    pub accepted_input_cut: Counter,
    pub checkpoint_id: Nullable<DomainId>,
    pub limits: Limits,
}
impl Validate for RpcRequestHarnessStartTurnBody {
    fn validate(&self) -> Result<(), ContractError> {
        self.run_spec.validate()?;
        self.lease.validate()?;
        self.turn_id.validate()?;
        self.accepted_input_cut.validate()?;
        self.checkpoint_id.validate()?;
        self.limits.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcRequestHarnessSubmitInputBody {
    pub run_id: DomainId,
    pub attempt_id: DomainId,
    pub command_id: DomainId,
    pub accepted_seq: Counter,
    pub payload_digest: Digest,
}
impl Validate for RpcRequestHarnessSubmitInputBody {
    fn validate(&self) -> Result<(), ContractError> {
        self.run_id.validate()?;
        self.attempt_id.validate()?;
        self.command_id.validate()?;
        self.accepted_seq.validate()?;
        self.payload_digest.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcRequestHarnessRequestPauseBody {
    pub run_id: DomainId,
    pub attempt_id: DomainId,
    pub pause_command_id: DomainId,
}
impl Validate for RpcRequestHarnessRequestPauseBody {
    fn validate(&self) -> Result<(), ContractError> {
        self.run_id.validate()?;
        self.attempt_id.validate()?;
        self.pause_command_id.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcRequestHarnessCancelBody {
    pub run_id: DomainId,
    pub attempt_id: DomainId,
    pub cancel_command_id: DomainId,
}
impl Validate for RpcRequestHarnessCancelBody {
    fn validate(&self) -> Result<(), ContractError> {
        self.run_id.validate()?;
        self.attempt_id.validate()?;
        self.cancel_command_id.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum RpcRequestHarnessPrepareCheckpointBodyReason {
    #[serde(rename = "turn_limit")]
    TurnLimit,
    #[serde(rename = "pause")]
    Pause,
    #[serde(rename = "transfer")]
    Transfer,
    #[serde(rename = "restore")]
    Restore,
    #[serde(rename = "context_limit")]
    ContextLimit,
}
impl Validate for RpcRequestHarnessPrepareCheckpointBodyReason {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcRequestHarnessPrepareCheckpointBody {
    pub run_id: DomainId,
    pub attempt_id: DomainId,
    pub event_cut: Counter,
    pub accepted_cut: Counter,
    pub coverage: FileCoverage,
    pub reason: RpcRequestHarnessPrepareCheckpointBodyReason,
}
impl Validate for RpcRequestHarnessPrepareCheckpointBody {
    fn validate(&self) -> Result<(), ContractError> {
        self.run_id.validate()?;
        self.attempt_id.validate()?;
        self.event_cut.validate()?;
        self.accepted_cut.validate()?;
        self.coverage.validate()?;
        self.reason.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcRequestHarnessResumeTurnBody {
    pub run_spec: RunSpec,
    pub lease: Lease,
    pub turn_id: DomainId,
    pub checkpoint_id: DomainId,
    pub accepted_input_cut: Counter,
    pub limits: Limits,
}
impl Validate for RpcRequestHarnessResumeTurnBody {
    fn validate(&self) -> Result<(), ContractError> {
        self.run_spec.validate()?;
        self.lease.validate()?;
        self.turn_id.validate()?;
        self.checkpoint_id.validate()?;
        self.accepted_input_cut.validate()?;
        self.limits.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcRequestHarnessSubscribeBody {
    pub run_id: DomainId,
    pub after_sequence: Counter,
    pub provisional_cursor: Nullable<Text<0, 2048>>,
}
impl Validate for RpcRequestHarnessSubscribeBody {
    fn validate(&self) -> Result<(), ContractError> {
        self.run_id.validate()?;
        self.after_sequence.validate()?;
        self.provisional_cursor.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(tag = "operation")]
#[serde(deny_unknown_fields)]
pub enum RpcRequest {
    #[serde(rename = "handshake")]
    Handshake {
        frame_kind: RequestFrameKind,
        request_id: DomainId,
        body: RpcRequestHandshakeBody,
    },
    #[serde(rename = "describe_capabilities")]
    DescribeCapabilities {
        frame_kind: RequestFrameKind,
        request_id: DomainId,
        body: RpcRequestDescribeCapabilitiesBody,
    },
    #[serde(rename = "submit_task")]
    SubmitTask {
        frame_kind: RequestFrameKind,
        request_id: DomainId,
        body: RpcRequestSubmitTaskBody,
    },
    #[serde(rename = "submit_command")]
    SubmitCommand {
        frame_kind: RequestFrameKind,
        request_id: DomainId,
        body: RpcRequestSubmitCommandBody,
    },
    #[serde(rename = "get_task_snapshot")]
    GetTaskSnapshot {
        frame_kind: RequestFrameKind,
        request_id: DomainId,
        body: RpcRequestGetTaskSnapshotBody,
    },
    #[serde(rename = "subscribe")]
    Subscribe {
        frame_kind: RequestFrameKind,
        request_id: DomainId,
        body: RpcRequestSubscribeBody,
    },
    #[serde(rename = "answer_clarification")]
    AnswerClarification {
        frame_kind: RequestFrameKind,
        request_id: DomainId,
        body: RpcRequestAnswerClarificationBody,
    },
    #[serde(rename = "review_artifact")]
    ReviewArtifact {
        frame_kind: RequestFrameKind,
        request_id: DomainId,
        body: RpcRequestReviewArtifactBody,
    },
    #[serde(rename = "decide_action")]
    DecideAction {
        frame_kind: RequestFrameKind,
        request_id: DomainId,
        body: RpcRequestDecideActionBody,
    },
    #[serde(rename = "get_artifact")]
    GetArtifact {
        frame_kind: RequestFrameKind,
        request_id: DomainId,
        body: RpcRequestGetArtifactBody,
    },
    #[serde(rename = "prepare_transfer")]
    PrepareTransfer {
        frame_kind: RequestFrameKind,
        request_id: DomainId,
        body: RpcRequestPrepareTransferBody,
    },
    #[serde(rename = "query_transfer")]
    QueryTransfer {
        frame_kind: RequestFrameKind,
        request_id: DomainId,
        body: RpcRequestQueryTransferBody,
    },
    #[serde(rename = "request_transfer_abort")]
    RequestTransferAbort {
        frame_kind: RequestFrameKind,
        request_id: DomainId,
        body: RpcRequestRequestTransferAbortBody,
    },
    #[serde(rename = "get_usage")]
    GetUsage {
        frame_kind: RequestFrameKind,
        request_id: DomainId,
        body: RpcRequestGetUsageBody,
    },
    #[serde(rename = "harness.describe_capabilities")]
    HarnessDescribeCapabilities {
        frame_kind: RequestFrameKind,
        request_id: DomainId,
        body: RpcRequestHarnessDescribeCapabilitiesBody,
    },
    #[serde(rename = "harness.start_turn")]
    HarnessStartTurn {
        frame_kind: RequestFrameKind,
        request_id: DomainId,
        body: RpcRequestHarnessStartTurnBody,
    },
    #[serde(rename = "harness.submit_input")]
    HarnessSubmitInput {
        frame_kind: RequestFrameKind,
        request_id: DomainId,
        body: RpcRequestHarnessSubmitInputBody,
    },
    #[serde(rename = "harness.request_pause")]
    HarnessRequestPause {
        frame_kind: RequestFrameKind,
        request_id: DomainId,
        body: RpcRequestHarnessRequestPauseBody,
    },
    #[serde(rename = "harness.cancel")]
    HarnessCancel {
        frame_kind: RequestFrameKind,
        request_id: DomainId,
        body: RpcRequestHarnessCancelBody,
    },
    #[serde(rename = "harness.prepare_checkpoint")]
    HarnessPrepareCheckpoint {
        frame_kind: RequestFrameKind,
        request_id: DomainId,
        body: RpcRequestHarnessPrepareCheckpointBody,
    },
    #[serde(rename = "harness.resume_turn")]
    HarnessResumeTurn {
        frame_kind: RequestFrameKind,
        request_id: DomainId,
        body: RpcRequestHarnessResumeTurnBody,
    },
    #[serde(rename = "harness.subscribe")]
    HarnessSubscribe {
        frame_kind: RequestFrameKind,
        request_id: DomainId,
        body: RpcRequestHarnessSubscribeBody,
    },
}
impl Validate for RpcRequest {
    fn validate(&self) -> Result<(), ContractError> {
        match self {
            Self::Handshake {
                frame_kind,
                request_id,
                body,
            } => {
                frame_kind.validate()?;
                request_id.validate()?;
                body.validate()?;
                Ok(())
            }
            Self::DescribeCapabilities {
                frame_kind,
                request_id,
                body,
            } => {
                frame_kind.validate()?;
                request_id.validate()?;
                body.validate()?;
                Ok(())
            }
            Self::SubmitTask {
                frame_kind,
                request_id,
                body,
            } => {
                frame_kind.validate()?;
                request_id.validate()?;
                body.validate()?;
                Ok(())
            }
            Self::SubmitCommand {
                frame_kind,
                request_id,
                body,
            } => {
                frame_kind.validate()?;
                request_id.validate()?;
                body.validate()?;
                Ok(())
            }
            Self::GetTaskSnapshot {
                frame_kind,
                request_id,
                body,
            } => {
                frame_kind.validate()?;
                request_id.validate()?;
                body.validate()?;
                Ok(())
            }
            Self::Subscribe {
                frame_kind,
                request_id,
                body,
            } => {
                frame_kind.validate()?;
                request_id.validate()?;
                body.validate()?;
                Ok(())
            }
            Self::AnswerClarification {
                frame_kind,
                request_id,
                body,
            } => {
                frame_kind.validate()?;
                request_id.validate()?;
                body.validate()?;
                Ok(())
            }
            Self::ReviewArtifact {
                frame_kind,
                request_id,
                body,
            } => {
                frame_kind.validate()?;
                request_id.validate()?;
                body.validate()?;
                Ok(())
            }
            Self::DecideAction {
                frame_kind,
                request_id,
                body,
            } => {
                frame_kind.validate()?;
                request_id.validate()?;
                body.validate()?;
                Ok(())
            }
            Self::GetArtifact {
                frame_kind,
                request_id,
                body,
            } => {
                frame_kind.validate()?;
                request_id.validate()?;
                body.validate()?;
                Ok(())
            }
            Self::PrepareTransfer {
                frame_kind,
                request_id,
                body,
            } => {
                frame_kind.validate()?;
                request_id.validate()?;
                body.validate()?;
                Ok(())
            }
            Self::QueryTransfer {
                frame_kind,
                request_id,
                body,
            } => {
                frame_kind.validate()?;
                request_id.validate()?;
                body.validate()?;
                Ok(())
            }
            Self::RequestTransferAbort {
                frame_kind,
                request_id,
                body,
            } => {
                frame_kind.validate()?;
                request_id.validate()?;
                body.validate()?;
                Ok(())
            }
            Self::GetUsage {
                frame_kind,
                request_id,
                body,
            } => {
                frame_kind.validate()?;
                request_id.validate()?;
                body.validate()?;
                Ok(())
            }
            Self::HarnessDescribeCapabilities {
                frame_kind,
                request_id,
                body,
            } => {
                frame_kind.validate()?;
                request_id.validate()?;
                body.validate()?;
                Ok(())
            }
            Self::HarnessStartTurn {
                frame_kind,
                request_id,
                body,
            } => {
                frame_kind.validate()?;
                request_id.validate()?;
                body.validate()?;
                Ok(())
            }
            Self::HarnessSubmitInput {
                frame_kind,
                request_id,
                body,
            } => {
                frame_kind.validate()?;
                request_id.validate()?;
                body.validate()?;
                Ok(())
            }
            Self::HarnessRequestPause {
                frame_kind,
                request_id,
                body,
            } => {
                frame_kind.validate()?;
                request_id.validate()?;
                body.validate()?;
                Ok(())
            }
            Self::HarnessCancel {
                frame_kind,
                request_id,
                body,
            } => {
                frame_kind.validate()?;
                request_id.validate()?;
                body.validate()?;
                Ok(())
            }
            Self::HarnessPrepareCheckpoint {
                frame_kind,
                request_id,
                body,
            } => {
                frame_kind.validate()?;
                request_id.validate()?;
                body.validate()?;
                Ok(())
            }
            Self::HarnessResumeTurn {
                frame_kind,
                request_id,
                body,
            } => {
                frame_kind.validate()?;
                request_id.validate()?;
                body.validate()?;
                Ok(())
            }
            Self::HarnessSubscribe {
                frame_kind,
                request_id,
                body,
            } => {
                frame_kind.validate()?;
                request_id.validate()?;
                body.validate()?;
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum RpcResponseHandshakeOkFrameKind {
    #[serde(rename = "response")]
    Response,
}
impl Validate for RpcResponseHandshakeOkFrameKind {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum RpcResponseHandshakeOkOperation {
    #[serde(rename = "handshake")]
    Handshake,
}
impl Validate for RpcResponseHandshakeOkOperation {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseHandshakeOkResult {
    pub selected_version: ProtocolVersion,
    pub supervisor_generation: PositiveCounter,
    pub schema_min: Text<0, 128>,
    pub schema_max: Text<0, 128>,
    pub capability_manifest_digest: Digest,
    pub max_frame_bytes: SmallInteger<1048576, 1048576>,
    pub authenticated_scope_id: DomainId,
}
impl Validate for RpcResponseHandshakeOkResult {
    fn validate(&self) -> Result<(), ContractError> {
        self.selected_version.validate()?;
        self.supervisor_generation.validate()?;
        self.schema_min.validate()?;
        self.schema_max.validate()?;
        self.capability_manifest_digest.validate()?;
        self.max_frame_bytes.validate()?;
        self.authenticated_scope_id.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseHandshakeOk {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseHandshakeOkOperation,
    pub ok: LiteralBool<true>,
    pub result: RpcResponseHandshakeOkResult,
}
impl Validate for RpcResponseHandshakeOk {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.result.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseHandshakeError {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseHandshakeOkOperation,
    pub ok: LiteralBool<false>,
    pub error: SafeError,
}
impl Validate for RpcResponseHandshakeError {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.error.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum RpcResponseDescribeCapabilitiesOkOperation {
    #[serde(rename = "describe_capabilities")]
    DescribeCapabilities,
}
impl Validate for RpcResponseDescribeCapabilitiesOkOperation {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseDescribeCapabilitiesOk {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseDescribeCapabilitiesOkOperation,
    pub ok: LiteralBool<true>,
    pub result: CapabilityDescription,
}
impl Validate for RpcResponseDescribeCapabilitiesOk {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.result.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseDescribeCapabilitiesError {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseDescribeCapabilitiesOkOperation,
    pub ok: LiteralBool<false>,
    pub error: SafeError,
}
impl Validate for RpcResponseDescribeCapabilitiesError {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.error.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum RpcResponseGetTaskSnapshotOkOperation {
    #[serde(rename = "get_task_snapshot")]
    GetTaskSnapshot,
}
impl Validate for RpcResponseGetTaskSnapshotOkOperation {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseGetTaskSnapshotOk {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseGetTaskSnapshotOkOperation,
    pub ok: LiteralBool<true>,
    pub result: TaskSnapshot,
}
impl Validate for RpcResponseGetTaskSnapshotOk {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.result.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseGetTaskSnapshotError {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseGetTaskSnapshotOkOperation,
    pub ok: LiteralBool<false>,
    pub error: SafeError,
}
impl Validate for RpcResponseGetTaskSnapshotError {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.error.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum RpcResponseSubscribeOkOperation {
    #[serde(rename = "subscribe")]
    Subscribe,
}
impl Validate for RpcResponseSubscribeOkOperation {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseSubscribeOk {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseSubscribeOkOperation,
    pub ok: LiteralBool<true>,
    pub result: Subscription,
}
impl Validate for RpcResponseSubscribeOk {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.result.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseSubscribeError {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseSubscribeOkOperation,
    pub ok: LiteralBool<false>,
    pub error: SafeError,
}
impl Validate for RpcResponseSubscribeError {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.error.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum RpcResponseGetArtifactOkOperation {
    #[serde(rename = "get_artifact")]
    GetArtifact,
}
impl Validate for RpcResponseGetArtifactOkOperation {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseGetArtifactOk {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseGetArtifactOkOperation,
    pub ok: LiteralBool<true>,
    pub result: ArtifactReadResult,
}
impl Validate for RpcResponseGetArtifactOk {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.result.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseGetArtifactError {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseGetArtifactOkOperation,
    pub ok: LiteralBool<false>,
    pub error: SafeError,
}
impl Validate for RpcResponseGetArtifactError {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.error.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum RpcResponseGetUsageOkOperation {
    #[serde(rename = "get_usage")]
    GetUsage,
}
impl Validate for RpcResponseGetUsageOkOperation {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseGetUsageOk {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseGetUsageOkOperation,
    pub ok: LiteralBool<true>,
    pub result: UsageReport,
}
impl Validate for RpcResponseGetUsageOk {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.result.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseGetUsageError {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseGetUsageOkOperation,
    pub ok: LiteralBool<false>,
    pub error: SafeError,
}
impl Validate for RpcResponseGetUsageError {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.error.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum RpcResponseHarnessDescribeCapabilitiesOkOperation {
    #[serde(rename = "harness.describe_capabilities")]
    HarnessDescribeCapabilities,
}
impl Validate for RpcResponseHarnessDescribeCapabilitiesOkOperation {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseHarnessDescribeCapabilitiesOk {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseHarnessDescribeCapabilitiesOkOperation,
    pub ok: LiteralBool<true>,
    pub result: CapabilityDescription,
}
impl Validate for RpcResponseHarnessDescribeCapabilitiesOk {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.result.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseHarnessDescribeCapabilitiesError {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseHarnessDescribeCapabilitiesOkOperation,
    pub ok: LiteralBool<false>,
    pub error: SafeError,
}
impl Validate for RpcResponseHarnessDescribeCapabilitiesError {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.error.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum RpcResponseHarnessStartTurnOkOperation {
    #[serde(rename = "harness.start_turn")]
    HarnessStartTurn,
}
impl Validate for RpcResponseHarnessStartTurnOkOperation {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseHarnessStartTurnOkResult {
    pub turn_id: DomainId,
    pub accepted: LiteralBool<true>,
}
impl Validate for RpcResponseHarnessStartTurnOkResult {
    fn validate(&self) -> Result<(), ContractError> {
        self.turn_id.validate()?;
        self.accepted.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseHarnessStartTurnOk {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseHarnessStartTurnOkOperation,
    pub ok: LiteralBool<true>,
    pub result: RpcResponseHarnessStartTurnOkResult,
}
impl Validate for RpcResponseHarnessStartTurnOk {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.result.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseHarnessStartTurnError {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseHarnessStartTurnOkOperation,
    pub ok: LiteralBool<false>,
    pub error: SafeError,
}
impl Validate for RpcResponseHarnessStartTurnError {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.error.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum RpcResponseHarnessResumeTurnOkOperation {
    #[serde(rename = "harness.resume_turn")]
    HarnessResumeTurn,
}
impl Validate for RpcResponseHarnessResumeTurnOkOperation {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseHarnessResumeTurnOkResult {
    pub turn_id: DomainId,
    pub accepted: LiteralBool<true>,
}
impl Validate for RpcResponseHarnessResumeTurnOkResult {
    fn validate(&self) -> Result<(), ContractError> {
        self.turn_id.validate()?;
        self.accepted.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseHarnessResumeTurnOk {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseHarnessResumeTurnOkOperation,
    pub ok: LiteralBool<true>,
    pub result: RpcResponseHarnessResumeTurnOkResult,
}
impl Validate for RpcResponseHarnessResumeTurnOk {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.result.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseHarnessResumeTurnError {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseHarnessResumeTurnOkOperation,
    pub ok: LiteralBool<false>,
    pub error: SafeError,
}
impl Validate for RpcResponseHarnessResumeTurnError {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.error.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum RpcResponseHarnessPrepareCheckpointOkOperation {
    #[serde(rename = "harness.prepare_checkpoint")]
    HarnessPrepareCheckpoint,
}
impl Validate for RpcResponseHarnessPrepareCheckpointOkOperation {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseHarnessPrepareCheckpointOk {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseHarnessPrepareCheckpointOkOperation,
    pub ok: LiteralBool<true>,
    pub result: Checkpoint,
}
impl Validate for RpcResponseHarnessPrepareCheckpointOk {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.result.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseHarnessPrepareCheckpointError {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseHarnessPrepareCheckpointOkOperation,
    pub ok: LiteralBool<false>,
    pub error: SafeError,
}
impl Validate for RpcResponseHarnessPrepareCheckpointError {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.error.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum RpcResponseHarnessSubscribeOkOperation {
    #[serde(rename = "harness.subscribe")]
    HarnessSubscribe,
}
impl Validate for RpcResponseHarnessSubscribeOkOperation {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseHarnessSubscribeOk {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseHarnessSubscribeOkOperation,
    pub ok: LiteralBool<true>,
    pub result: Subscription,
}
impl Validate for RpcResponseHarnessSubscribeOk {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.result.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseHarnessSubscribeError {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseHarnessSubscribeOkOperation,
    pub ok: LiteralBool<false>,
    pub error: SafeError,
}
impl Validate for RpcResponseHarnessSubscribeError {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.error.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum RpcResponseHarnessRequestPauseOkOperation {
    #[serde(rename = "harness.request_pause")]
    HarnessRequestPause,
}
impl Validate for RpcResponseHarnessRequestPauseOkOperation {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(tag = "kind")]
#[serde(deny_unknown_fields)]
pub enum RpcResponseHarnessRequestPauseOkResult {
    #[serde(rename = "paused")]
    Paused {
        reason: TurnOutcomeYieldedReason,
        checkpoint_id: DomainId,
    },
    #[serde(rename = "failed")]
    Failed {
        error: SafeError,
        retry_class: TurnOutcomeFailedRetryClass,
        checkpoint_id: Nullable<DomainId>,
    },
}
impl Validate for RpcResponseHarnessRequestPauseOkResult {
    fn validate(&self) -> Result<(), ContractError> {
        match self {
            Self::Paused {
                reason,
                checkpoint_id,
            } => {
                reason.validate()?;
                checkpoint_id.validate()?;
                Ok(())
            }
            Self::Failed {
                error,
                retry_class,
                checkpoint_id,
            } => {
                error.validate()?;
                retry_class.validate()?;
                checkpoint_id.validate()?;
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseHarnessRequestPauseOk {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseHarnessRequestPauseOkOperation,
    pub ok: LiteralBool<true>,
    pub result: RpcResponseHarnessRequestPauseOkResult,
}
impl Validate for RpcResponseHarnessRequestPauseOk {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.result.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseHarnessRequestPauseError {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseHarnessRequestPauseOkOperation,
    pub ok: LiteralBool<false>,
    pub error: SafeError,
}
impl Validate for RpcResponseHarnessRequestPauseError {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.error.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum RpcResponseHarnessCancelOkOperation {
    #[serde(rename = "harness.cancel")]
    HarnessCancel,
}
impl Validate for RpcResponseHarnessCancelOkOperation {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(tag = "kind")]
#[serde(deny_unknown_fields)]
pub enum RpcResponseHarnessCancelOkResult {
    #[serde(rename = "cancelled")]
    Cancelled {
        checkpoint_id: Nullable<DomainId>,
        cleanup_refs: List<DomainId, 0, 100>,
        unresolved_receipt_refs: List<DomainId, 0, 100>,
    },
    #[serde(rename = "failed")]
    Failed {
        error: SafeError,
        retry_class: TurnOutcomeFailedRetryClass,
        checkpoint_id: Nullable<DomainId>,
    },
}
impl Validate for RpcResponseHarnessCancelOkResult {
    fn validate(&self) -> Result<(), ContractError> {
        match self {
            Self::Cancelled {
                checkpoint_id,
                cleanup_refs,
                unresolved_receipt_refs,
            } => {
                checkpoint_id.validate()?;
                cleanup_refs.validate()?;
                unresolved_receipt_refs.validate()?;
                Ok(())
            }
            Self::Failed {
                error,
                retry_class,
                checkpoint_id,
            } => {
                error.validate()?;
                retry_class.validate()?;
                checkpoint_id.validate()?;
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseHarnessCancelOk {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseHarnessCancelOkOperation,
    pub ok: LiteralBool<true>,
    pub result: RpcResponseHarnessCancelOkResult,
}
impl Validate for RpcResponseHarnessCancelOk {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.result.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseHarnessCancelError {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseHarnessCancelOkOperation,
    pub ok: LiteralBool<false>,
    pub error: SafeError,
}
impl Validate for RpcResponseHarnessCancelError {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.error.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum RpcResponseHarnessSubmitInputOkOperation {
    #[serde(rename = "harness.submit_input")]
    HarnessSubmitInput,
}
impl Validate for RpcResponseHarnessSubmitInputOkOperation {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseHarnessSubmitInputOk {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseHarnessSubmitInputOkOperation,
    pub ok: LiteralBool<true>,
    pub result: InputDisposition,
}
impl Validate for RpcResponseHarnessSubmitInputOk {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.result.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseHarnessSubmitInputError {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseHarnessSubmitInputOkOperation,
    pub ok: LiteralBool<false>,
    pub error: SafeError,
}
impl Validate for RpcResponseHarnessSubmitInputError {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.error.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum RpcResponseSubmitTaskOkOperation {
    #[serde(rename = "submit_task")]
    SubmitTask,
}
impl Validate for RpcResponseSubmitTaskOkOperation {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseSubmitTaskOk {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseSubmitTaskOkOperation,
    pub ok: LiteralBool<true>,
    pub result: CommandReceipt,
}
impl Validate for RpcResponseSubmitTaskOk {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.result.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseSubmitTaskError {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseSubmitTaskOkOperation,
    pub ok: LiteralBool<false>,
    pub error: SafeError,
}
impl Validate for RpcResponseSubmitTaskError {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.error.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum RpcResponseSubmitCommandOkOperation {
    #[serde(rename = "submit_command")]
    SubmitCommand,
}
impl Validate for RpcResponseSubmitCommandOkOperation {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseSubmitCommandOk {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseSubmitCommandOkOperation,
    pub ok: LiteralBool<true>,
    pub result: CommandReceipt,
}
impl Validate for RpcResponseSubmitCommandOk {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.result.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseSubmitCommandError {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseSubmitCommandOkOperation,
    pub ok: LiteralBool<false>,
    pub error: SafeError,
}
impl Validate for RpcResponseSubmitCommandError {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.error.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseAnswerClarificationOk {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcRequestAnswerClarificationBodyKind,
    pub ok: LiteralBool<true>,
    pub result: CommandReceipt,
}
impl Validate for RpcResponseAnswerClarificationOk {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.result.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseAnswerClarificationError {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcRequestAnswerClarificationBodyKind,
    pub ok: LiteralBool<false>,
    pub error: SafeError,
}
impl Validate for RpcResponseAnswerClarificationError {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.error.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseReviewArtifactOk {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcRequestReviewArtifactBodyKind,
    pub ok: LiteralBool<true>,
    pub result: CommandReceipt,
}
impl Validate for RpcResponseReviewArtifactOk {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.result.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseReviewArtifactError {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcRequestReviewArtifactBodyKind,
    pub ok: LiteralBool<false>,
    pub error: SafeError,
}
impl Validate for RpcResponseReviewArtifactError {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.error.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseDecideActionOk {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcRequestDecideActionBodyKind,
    pub ok: LiteralBool<true>,
    pub result: CommandReceipt,
}
impl Validate for RpcResponseDecideActionOk {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.result.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseDecideActionError {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcRequestDecideActionBodyKind,
    pub ok: LiteralBool<false>,
    pub error: SafeError,
}
impl Validate for RpcResponseDecideActionError {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.error.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum RpcResponsePrepareTransferOkOperation {
    #[serde(rename = "prepare_transfer")]
    PrepareTransfer,
}
impl Validate for RpcResponsePrepareTransferOkOperation {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponsePrepareTransferOk {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponsePrepareTransferOkOperation,
    pub ok: LiteralBool<true>,
    pub result: TransferReceipt,
}
impl Validate for RpcResponsePrepareTransferOk {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.result.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponsePrepareTransferError {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponsePrepareTransferOkOperation,
    pub ok: LiteralBool<false>,
    pub error: SafeError,
}
impl Validate for RpcResponsePrepareTransferError {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.error.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum RpcResponseQueryTransferOkOperation {
    #[serde(rename = "query_transfer")]
    QueryTransfer,
}
impl Validate for RpcResponseQueryTransferOkOperation {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseQueryTransferOk {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseQueryTransferOkOperation,
    pub ok: LiteralBool<true>,
    pub result: TransferReceipt,
}
impl Validate for RpcResponseQueryTransferOk {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.result.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseQueryTransferError {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseQueryTransferOkOperation,
    pub ok: LiteralBool<false>,
    pub error: SafeError,
}
impl Validate for RpcResponseQueryTransferError {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.error.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum RpcResponseRequestTransferAbortOkOperation {
    #[serde(rename = "request_transfer_abort")]
    RequestTransferAbort,
}
impl Validate for RpcResponseRequestTransferAbortOkOperation {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseRequestTransferAbortOk {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseRequestTransferAbortOkOperation,
    pub ok: LiteralBool<true>,
    pub result: TransferReceipt,
}
impl Validate for RpcResponseRequestTransferAbortOk {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.result.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RpcResponseRequestTransferAbortError {
    pub frame_kind: RpcResponseHandshakeOkFrameKind,
    pub request_id: DomainId,
    pub operation: RpcResponseRequestTransferAbortOkOperation,
    pub ok: LiteralBool<false>,
    pub error: SafeError,
}
impl Validate for RpcResponseRequestTransferAbortError {
    fn validate(&self) -> Result<(), ContractError> {
        self.frame_kind.validate()?;
        self.request_id.validate()?;
        self.operation.validate()?;
        self.ok.validate()?;
        self.error.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(untagged)]
pub enum RpcResponse {
    HandshakeOk(Box<RpcResponseHandshakeOk>),
    HandshakeError(Box<RpcResponseHandshakeError>),
    DescribeCapabilitiesOk(Box<RpcResponseDescribeCapabilitiesOk>),
    DescribeCapabilitiesError(Box<RpcResponseDescribeCapabilitiesError>),
    GetTaskSnapshotOk(Box<RpcResponseGetTaskSnapshotOk>),
    GetTaskSnapshotError(Box<RpcResponseGetTaskSnapshotError>),
    SubscribeOk(Box<RpcResponseSubscribeOk>),
    SubscribeError(Box<RpcResponseSubscribeError>),
    GetArtifactOk(Box<RpcResponseGetArtifactOk>),
    GetArtifactError(Box<RpcResponseGetArtifactError>),
    GetUsageOk(Box<RpcResponseGetUsageOk>),
    GetUsageError(Box<RpcResponseGetUsageError>),
    HarnessDescribeCapabilitiesOk(Box<RpcResponseHarnessDescribeCapabilitiesOk>),
    HarnessDescribeCapabilitiesError(Box<RpcResponseHarnessDescribeCapabilitiesError>),
    HarnessStartTurnOk(Box<RpcResponseHarnessStartTurnOk>),
    HarnessStartTurnError(Box<RpcResponseHarnessStartTurnError>),
    HarnessResumeTurnOk(Box<RpcResponseHarnessResumeTurnOk>),
    HarnessResumeTurnError(Box<RpcResponseHarnessResumeTurnError>),
    HarnessPrepareCheckpointOk(Box<RpcResponseHarnessPrepareCheckpointOk>),
    HarnessPrepareCheckpointError(Box<RpcResponseHarnessPrepareCheckpointError>),
    HarnessSubscribeOk(Box<RpcResponseHarnessSubscribeOk>),
    HarnessSubscribeError(Box<RpcResponseHarnessSubscribeError>),
    HarnessRequestPauseOk(Box<RpcResponseHarnessRequestPauseOk>),
    HarnessRequestPauseError(Box<RpcResponseHarnessRequestPauseError>),
    HarnessCancelOk(Box<RpcResponseHarnessCancelOk>),
    HarnessCancelError(Box<RpcResponseHarnessCancelError>),
    HarnessSubmitInputOk(Box<RpcResponseHarnessSubmitInputOk>),
    HarnessSubmitInputError(Box<RpcResponseHarnessSubmitInputError>),
    SubmitTaskOk(Box<RpcResponseSubmitTaskOk>),
    SubmitTaskError(Box<RpcResponseSubmitTaskError>),
    SubmitCommandOk(Box<RpcResponseSubmitCommandOk>),
    SubmitCommandError(Box<RpcResponseSubmitCommandError>),
    AnswerClarificationOk(Box<RpcResponseAnswerClarificationOk>),
    AnswerClarificationError(Box<RpcResponseAnswerClarificationError>),
    ReviewArtifactOk(Box<RpcResponseReviewArtifactOk>),
    ReviewArtifactError(Box<RpcResponseReviewArtifactError>),
    DecideActionOk(Box<RpcResponseDecideActionOk>),
    DecideActionError(Box<RpcResponseDecideActionError>),
    PrepareTransferOk(Box<RpcResponsePrepareTransferOk>),
    PrepareTransferError(Box<RpcResponsePrepareTransferError>),
    QueryTransferOk(Box<RpcResponseQueryTransferOk>),
    QueryTransferError(Box<RpcResponseQueryTransferError>),
    RequestTransferAbortOk(Box<RpcResponseRequestTransferAbortOk>),
    RequestTransferAbortError(Box<RpcResponseRequestTransferAbortError>),
}
impl Validate for RpcResponse {
    fn validate(&self) -> Result<(), ContractError> {
        match self {
            Self::HandshakeOk(v) => v.validate(),
            Self::HandshakeError(v) => v.validate(),
            Self::DescribeCapabilitiesOk(v) => v.validate(),
            Self::DescribeCapabilitiesError(v) => v.validate(),
            Self::GetTaskSnapshotOk(v) => v.validate(),
            Self::GetTaskSnapshotError(v) => v.validate(),
            Self::SubscribeOk(v) => v.validate(),
            Self::SubscribeError(v) => v.validate(),
            Self::GetArtifactOk(v) => v.validate(),
            Self::GetArtifactError(v) => v.validate(),
            Self::GetUsageOk(v) => v.validate(),
            Self::GetUsageError(v) => v.validate(),
            Self::HarnessDescribeCapabilitiesOk(v) => v.validate(),
            Self::HarnessDescribeCapabilitiesError(v) => v.validate(),
            Self::HarnessStartTurnOk(v) => v.validate(),
            Self::HarnessStartTurnError(v) => v.validate(),
            Self::HarnessResumeTurnOk(v) => v.validate(),
            Self::HarnessResumeTurnError(v) => v.validate(),
            Self::HarnessPrepareCheckpointOk(v) => v.validate(),
            Self::HarnessPrepareCheckpointError(v) => v.validate(),
            Self::HarnessSubscribeOk(v) => v.validate(),
            Self::HarnessSubscribeError(v) => v.validate(),
            Self::HarnessRequestPauseOk(v) => v.validate(),
            Self::HarnessRequestPauseError(v) => v.validate(),
            Self::HarnessCancelOk(v) => v.validate(),
            Self::HarnessCancelError(v) => v.validate(),
            Self::HarnessSubmitInputOk(v) => v.validate(),
            Self::HarnessSubmitInputError(v) => v.validate(),
            Self::SubmitTaskOk(v) => v.validate(),
            Self::SubmitTaskError(v) => v.validate(),
            Self::SubmitCommandOk(v) => v.validate(),
            Self::SubmitCommandError(v) => v.validate(),
            Self::AnswerClarificationOk(v) => v.validate(),
            Self::AnswerClarificationError(v) => v.validate(),
            Self::ReviewArtifactOk(v) => v.validate(),
            Self::ReviewArtifactError(v) => v.validate(),
            Self::DecideActionOk(v) => v.validate(),
            Self::DecideActionError(v) => v.validate(),
            Self::PrepareTransferOk(v) => v.validate(),
            Self::PrepareTransferError(v) => v.validate(),
            Self::QueryTransferOk(v) => v.validate(),
            Self::QueryTransferError(v) => v.validate(),
            Self::RequestTransferAbortOk(v) => v.validate(),
            Self::RequestTransferAbortError(v) => v.validate(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum RpcEventCommittedFrameKind {
    #[serde(rename = "event")]
    Event,
}
impl Validate for RpcEventCommittedFrameKind {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum RpcEventProvisionalKind {
    #[serde(rename = "model_text")]
    ModelText,
    #[serde(rename = "stdout")]
    Stdout,
    #[serde(rename = "stderr")]
    Stderr,
    #[serde(rename = "gap")]
    Gap,
}
impl Validate for RpcEventProvisionalKind {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(tag = "durability")]
#[serde(deny_unknown_fields)]
pub enum RpcEvent {
    #[serde(rename = "committed")]
    Committed {
        frame_kind: RpcEventCommittedFrameKind,
        subscription_id: DomainId,
        event: Box<Event>,
    },
    #[serde(rename = "provisional")]
    Provisional {
        frame_kind: RpcEventCommittedFrameKind,
        subscription_id: DomainId,
        run_id: DomainId,
        cursor: Text<0, 2048>,
        kind: RpcEventProvisionalKind,
        text: Text<0, 32768>,
        truncated: bool,
    },
}
impl Validate for RpcEvent {
    fn validate(&self) -> Result<(), ContractError> {
        match self {
            Self::Committed {
                frame_kind,
                subscription_id,
                event,
            } => {
                frame_kind.validate()?;
                subscription_id.validate()?;
                event.validate()?;
                Ok(())
            }
            Self::Provisional {
                frame_kind,
                subscription_id,
                run_id,
                cursor,
                kind,
                text,
                truncated,
            } => {
                frame_kind.validate()?;
                subscription_id.validate()?;
                run_id.validate()?;
                cursor.validate()?;
                kind.validate()?;
                text.validate()?;
                truncated.validate()?;
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(tag = "kind")]
#[serde(deny_unknown_fields)]
pub enum NormalizedModelResponseItemsItem {
    #[serde(rename = "text")]
    Text { text: Text<0, 262144> },
    #[serde(rename = "tool_proposal")]
    ToolProposal {
        provider_call_id: Text<0, 256>,
        binding_id: DomainId,
        arguments: JsonObject<128>,
    },
    #[serde(rename = "opaque_continuation")]
    OpaqueContinuation {
        provider_profile_id: DomainId,
        compatibility_digest: Digest,
        blob_ref: DomainId,
    },
}
impl Validate for NormalizedModelResponseItemsItem {
    fn validate(&self) -> Result<(), ContractError> {
        match self {
            Self::Text { text } => {
                text.validate()?;
                Ok(())
            }
            Self::ToolProposal {
                provider_call_id,
                binding_id,
                arguments,
            } => {
                provider_call_id.validate()?;
                binding_id.validate()?;
                arguments.validate()?;
                crate::json::bounded_json(arguments.as_map(), 262_144, "tool argument bytes")?;
                Ok(())
            }
            Self::OpaqueContinuation {
                provider_profile_id,
                compatibility_digest,
                blob_ref,
            } => {
                provider_profile_id.validate()?;
                compatibility_digest.validate()?;
                blob_ref.validate()?;
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum NormalizedModelResponseFinish {
    #[serde(rename = "tools")]
    Tools,
    #[serde(rename = "final")]
    Final,
    #[serde(rename = "length")]
    Length,
    #[serde(rename = "refused")]
    Refused,
    #[serde(rename = "interrupted")]
    Interrupted,
    #[serde(rename = "failed")]
    Failed,
}
impl Validate for NormalizedModelResponseFinish {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct NormalizedModelResponse {
    pub model_call_id: DomainId,
    pub step_id: DomainId,
    pub items: List<NormalizedModelResponseItemsItem, 0, 100>,
    pub finish: NormalizedModelResponseFinish,
    pub usage_refs: List<DomainId, 0, 100>,
    pub diagnostic_error: Nullable<SafeError>,
}
impl Validate for NormalizedModelResponse {
    fn validate(&self) -> Result<(), ContractError> {
        self.model_call_id.validate()?;
        self.step_id.validate()?;
        self.items.validate()?;
        self.finish.validate()?;
        self.usage_refs.validate()?;
        self.diagnostic_error.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum ContextManifestMembersItemKind {
    #[serde(rename = "instruction")]
    Instruction,
    #[serde(rename = "accepted_input")]
    AcceptedInput,
    #[serde(rename = "exact_state")]
    ExactState,
    #[serde(rename = "skill")]
    Skill,
    #[serde(rename = "memory")]
    Memory,
    #[serde(rename = "source")]
    Source,
    #[serde(rename = "history")]
    History,
}
impl Validate for ContextManifestMembersItemKind {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct ContextManifestMembersItem {
    pub kind: ContextManifestMembersItemKind,
    pub version_ref: DomainId,
    pub sha256: Digest,
    pub token_count: SmallInteger<0, 1000000>,
}
impl Validate for ContextManifestMembersItem {
    fn validate(&self) -> Result<(), ContractError> {
        self.kind.validate()?;
        self.version_ref.validate()?;
        self.sha256.validate()?;
        self.token_count.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct ContextManifest {
    pub manifest_id: DomainId,
    pub run_id: DomainId,
    pub task_revision_id: DomainId,
    pub accepted_seq: Counter,
    pub acting_principal_id: DomainId,
    pub audience_policy_id: DomainId,
    pub permission_generation: PositiveCounter,
    pub deletion_generation: Counter,
    pub members: List<ContextManifestMembersItem, 0, 1000>,
    pub omission_codes: List<Text<0, 128>, 0, 100>,
    pub input_token_count: SmallInteger<0, 1000000>,
    pub reserved_output_tokens: SmallInteger<1, 1000000>,
    pub assembled_at: Instant,
}
impl Validate for ContextManifest {
    fn validate(&self) -> Result<(), ContractError> {
        self.manifest_id.validate()?;
        self.run_id.validate()?;
        self.task_revision_id.validate()?;
        self.accepted_seq.validate()?;
        self.acting_principal_id.validate()?;
        self.audience_policy_id.validate()?;
        self.permission_generation.validate()?;
        self.deletion_generation.validate()?;
        self.members.validate()?;
        self.omission_codes.validate()?;
        self.input_token_count.validate()?;
        self.reserved_output_tokens.validate()?;
        self.assembled_at.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
pub enum InputDispositionDisposition {
    #[serde(rename = "applied")]
    Applied,
    #[serde(rename = "deferred")]
    Deferred,
    #[serde(rename = "superseded")]
    Superseded,
    #[serde(rename = "rejected")]
    Rejected,
}
impl Validate for InputDispositionDisposition {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct InputDisposition {
    pub command_id: DomainId,
    pub accepted_seq: Counter,
    pub disposition: InputDispositionDisposition,
    pub boundary_ref: DomainId,
}
impl Validate for InputDisposition {
    fn validate(&self) -> Result<(), ContractError> {
        self.command_id.validate()?;
        self.accepted_seq.validate()?;
        self.disposition.validate()?;
        self.boundary_ref.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "typescript-export", derive(ts_rs::TS))]
#[serde(tag = "message_kind")]
#[serde(deny_unknown_fields)]
pub enum WireMessage {
    #[serde(rename = "command")]
    Command {
        schema_version: ProtocolVersion,
        payload: Box<Command>,
    },
    #[serde(rename = "command_receipt")]
    CommandReceipt {
        schema_version: ProtocolVersion,
        payload: Box<CommandReceipt>,
    },
    #[serde(rename = "lease")]
    Lease {
        schema_version: ProtocolVersion,
        payload: Box<Lease>,
    },
    #[serde(rename = "step_binding")]
    StepBinding {
        schema_version: ProtocolVersion,
        payload: Box<StepBinding>,
    },
    #[serde(rename = "trusted_context")]
    TrustedContext {
        schema_version: ProtocolVersion,
        payload: Box<TrustedContext>,
    },
    #[serde(rename = "run_spec")]
    RunSpec {
        schema_version: ProtocolVersion,
        payload: Box<RunSpec>,
    },
    #[serde(rename = "event")]
    Event {
        schema_version: ProtocolVersion,
        payload: Box<Event>,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        schema_version: ProtocolVersion,
        payload: Box<ToolResult>,
    },
    #[serde(rename = "wait_spec")]
    WaitSpec {
        schema_version: ProtocolVersion,
        payload: Box<WaitSpec>,
    },
    #[serde(rename = "checkpoint")]
    Checkpoint {
        schema_version: ProtocolVersion,
        payload: Box<Checkpoint>,
    },
    #[serde(rename = "turn_outcome")]
    TurnOutcome {
        schema_version: ProtocolVersion,
        payload: Box<TurnOutcome>,
    },
    #[serde(rename = "rpc_request")]
    RpcRequest {
        schema_version: ProtocolVersion,
        payload: Box<RpcRequest>,
    },
    #[serde(rename = "rpc_response")]
    RpcResponse {
        schema_version: ProtocolVersion,
        payload: Box<RpcResponse>,
    },
    #[serde(rename = "rpc_event")]
    RpcEvent {
        schema_version: ProtocolVersion,
        payload: Box<RpcEvent>,
    },
    #[serde(rename = "normalized_model_response")]
    NormalizedModelResponse {
        schema_version: ProtocolVersion,
        payload: Box<NormalizedModelResponse>,
    },
    #[serde(rename = "context_manifest")]
    ContextManifest {
        schema_version: ProtocolVersion,
        payload: Box<ContextManifest>,
    },
}
impl Validate for WireMessage {
    fn validate(&self) -> Result<(), ContractError> {
        match self {
            Self::Command {
                schema_version,
                payload,
            } => {
                schema_version.validate()?;
                payload.validate()?;
                Ok(())
            }
            Self::CommandReceipt {
                schema_version,
                payload,
            } => {
                schema_version.validate()?;
                payload.validate()?;
                Ok(())
            }
            Self::Lease {
                schema_version,
                payload,
            } => {
                schema_version.validate()?;
                payload.validate()?;
                Ok(())
            }
            Self::StepBinding {
                schema_version,
                payload,
            } => {
                schema_version.validate()?;
                payload.validate()?;
                Ok(())
            }
            Self::TrustedContext {
                schema_version,
                payload,
            } => {
                schema_version.validate()?;
                payload.validate()?;
                Ok(())
            }
            Self::RunSpec {
                schema_version,
                payload,
            } => {
                schema_version.validate()?;
                payload.validate()?;
                Ok(())
            }
            Self::Event {
                schema_version,
                payload,
            } => {
                schema_version.validate()?;
                payload.validate()?;
                Ok(())
            }
            Self::ToolResult {
                schema_version,
                payload,
            } => {
                schema_version.validate()?;
                payload.validate()?;
                Ok(())
            }
            Self::WaitSpec {
                schema_version,
                payload,
            } => {
                schema_version.validate()?;
                payload.validate()?;
                Ok(())
            }
            Self::Checkpoint {
                schema_version,
                payload,
            } => {
                schema_version.validate()?;
                payload.validate()?;
                Ok(())
            }
            Self::TurnOutcome {
                schema_version,
                payload,
            } => {
                schema_version.validate()?;
                payload.validate()?;
                Ok(())
            }
            Self::RpcRequest {
                schema_version,
                payload,
            } => {
                schema_version.validate()?;
                payload.validate()?;
                Ok(())
            }
            Self::RpcResponse {
                schema_version,
                payload,
            } => {
                schema_version.validate()?;
                payload.validate()?;
                Ok(())
            }
            Self::RpcEvent {
                schema_version,
                payload,
            } => {
                schema_version.validate()?;
                payload.validate()?;
                Ok(())
            }
            Self::NormalizedModelResponse {
                schema_version,
                payload,
            } => {
                schema_version.validate()?;
                payload.validate()?;
                Ok(())
            }
            Self::ContextManifest {
                schema_version,
                payload,
            } => {
                schema_version.validate()?;
                payload.validate()?;
                Ok(())
            }
        }
    }
}
