// GENERATED FILE - DO NOT EDIT.
//
// Source of truth: crates/contracts (Rust). Regenerate with:
//   node tools/contracts/generate.mjs
// Verify freshness with:
//   node tools/contracts/generate.mjs --check
//
// Runtime validation uses the generated wire-validator.mjs through validate.ts.
// Types alone cannot reject invalid fields, duplicate JSON keys or dates.

export type ArtifactReadResult = { artifact: ArtifactRef, media_type: string, content_handle: string | null, next_cursor: string | null, truncated: boolean, };

export type ArtifactRef = { artifact_id: string, version_id: string, sha256: string, byte_size: string, };

export type Authority = "local" | "server" | "transfer_staging";

export type BoundData = { binding_id: string, value: unknown, };

export type CapabilityDescription = { protocol_version: ProtocolVersion, build_digest: string, schema_versions: Array<string>, operation_names: Array<string>, profiles: Array<CapabilityDescriptionProfilesItem>, capability_manifest_digest: string, };

export type CapabilityDescriptionProfilesItem = { id: string, enabled: boolean, reason: string | null, };

export type Checkpoint = { schema_version: ProtocolVersion, checkpoint_id: string, run_id: string, attempt_id: string, epoch: string, event_seq: string, accepted_seq: string, applied_seq: string, task_revision_id: string, session_id: string, branch_id: string, plan_version_id: string | null, graph_version_id: string | null, pending_decision_ids: Array<string>, unresolved_invocation_ids: Array<string>, committed_model_result_refs: Array<string>, source_memory_version_refs: Array<string>, manifest_id: string, manifest_digest: string, file_coverage: FileCoverage, file_gap_description: string | null, process_disposition_refs: Array<string>, environment_recipe_digest: string | null, permission_generation: string, deletion_generation: string, usage_refs: Array<string>, next_continuation_ref: string, };

export type Command = { "kind": "steer", command_id: string, run_id: string, idempotency_key: string, payload: CommandSteerPayload, } | { "kind": "revise_objective", command_id: string, run_id: string, idempotency_key: string, payload: CommandReviseObjectivePayload, } | { "kind": "pause", command_id: string, run_id: string, idempotency_key: string, payload: CommandPausePayload, } | { "kind": "resume", command_id: string, run_id: string, idempotency_key: string, payload: CommandResumePayload, } | { "kind": "stop", command_id: string, run_id: string, idempotency_key: string, payload: CommandStopPayload, } | { "kind": "answer_clarification", command_id: string, run_id: string, idempotency_key: string, payload: CommandAnswerClarificationPayload, } | { "kind": "review_artifact", command_id: string, run_id: string, idempotency_key: string, payload: CommandReviewArtifactPayload, } | { "kind": "decide_action", command_id: string, run_id: string, idempotency_key: string, payload: CommandDecideActionPayload, };

export type CommandAnswerClarificationPayload = { request_id: string, request_revision: string, answers: Array<CommandAnswerClarificationPayloadAnswersItem>, };

export type CommandAnswerClarificationPayloadAnswersItem = { question_id: string, answer: string, };

export type CommandDecideActionPayload = { decision_id: string, decision_revision: string, invocation_id: string, target_digest: string, payload_digest: string, response: CommandDecideActionPayloadResponse, };

export type CommandDecideActionPayloadResponse = "approve" | "reject";

export type CommandPausePayload = { expected_revision: string, reason: string, };

export type CommandReceipt = { "kind": "pending", command_id: string, authority: Authority, reason: PendingReason, } | { "kind": "accepted", command_id: string, authority: Authority, run_id: string, accepted_seq: string, event_seq: string, row_version: string, disposition_ref: string | null, task_id: string, task_revision_id: string, } | { "kind": "applied", command_id: string, authority: Authority, run_id: string, accepted_seq: string, event_seq: string, row_version: string, disposition_ref: string, task_id: string, task_revision_id: string, } | { "kind": "deferred", command_id: string, authority: Authority, run_id: string, accepted_seq: string, event_seq: string, row_version: string, disposition_ref: string, task_id: string, task_revision_id: string, } | { "kind": "superseded", command_id: string, authority: Authority, run_id: string, accepted_seq: string, event_seq: string, row_version: string, disposition_ref: string, task_id: string, task_revision_id: string, } | { "kind": "rejected", command_id: string, run_id: string, error: SafeError, current_revision: string | null, } | { "kind": "conflict", command_id: string, run_id: string, error: SafeError, current_revision: string | null, } | { "kind": "terminal", command_id: string, run_id: string, state: TerminalRunState, row_version: string, };

export type CommandResumePayload = { expected_revision: string, checkpoint_id: string, };

export type CommandReviewArtifactPayload = { request_id: string, request_revision: string, artifact_id: string, version_id: string, check_refs: Array<string>, response: CommandReviewArtifactPayloadResponse, };

export type CommandReviewArtifactPayloadResponse = "accept" | "reject" | "revise";

export type CommandReviseObjectivePayload = { objective_version_id: string, check_refs: Array<string>, expected_revision: string, };

export type CommandSteerPayload = { text: string, expected_revision: string, };

export type CommandStopPayload = { reason: string, };

export type ContextManifest = { manifest_id: string, run_id: string, task_revision_id: string, accepted_seq: string, acting_principal_id: string, audience_policy_id: string, permission_generation: string, deletion_generation: string, members: Array<ContextManifestMembersItem>, omission_codes: Array<string>, input_token_count: number, reserved_output_tokens: number, assembled_at: string, };

export type ContextManifestMembersItem = { kind: ContextManifestMembersItemKind, version_ref: string, sha256: string, token_count: number, };

export type ContextManifestMembersItemKind = "instruction" | "accepted_input" | "exact_state" | "skill" | "memory" | "source" | "history";

export type ErrorCode = "INVALID_MESSAGE" | "INVALID_ARGUMENT" | "UNSUPPORTED" | "INCOMPATIBLE" | "UNAUTHENTICATED" | "DENIED" | "NOT_FOUND_OR_DENIED" | "CONFLICT" | "IDEMPOTENCY_MISMATCH" | "STALE_AUTHORITY" | "CANCELLED" | "TIMEOUT" | "LOST" | "UNKNOWN_OUTCOME" | "RESOURCE_LIMIT" | "IO_FAILURE" | "RATE_LIMITED" | "UNAVAILABLE" | "NEEDS_RECONCILIATION";

export type Event = { schema_version: ProtocolVersion, event_id: string, workspace_id: string, run_id: string, attempt_id: string | null, turn_id: string | null, epoch: string, seq: string, event_type: EventType, command_id: string | null, invocation_id: string | null, causal_id: string | null, occurred_at: string, recorded_at: string, actor_id: string, payload_ref: string | null, };

export type EventType = "artifact.committed" | "attempt.claimed" | "attempt.lost" | "check.completed" | "checkpoint.committed" | "checkpoint.rejected" | "control.accepted" | "control.applied" | "control.deferred" | "control.superseded" | "decision.recorded" | "decision.requested" | "handoff.aborted" | "handoff.committed" | "handoff.prepared" | "input.accepted" | "model.completed" | "model.interrupted" | "model.requested" | "observation.accepted" | "process.exited" | "process.lost" | "process.output_gap" | "process.started" | "run.accepted" | "run.cancelled" | "run.completed" | "run.failed" | "run.transitioned" | "tool.authorized" | "tool.dispatched" | "tool.proposed" | "tool.settled" | "usage.recorded" | "wait.registered" | "wait.resolved";

export type ExecutionAuthority = "local" | "server";

export type FileCoverage = "coherent_snapshot" | "verified_per_file" | "no_files";

export type InputDisposition = { command_id: string, accepted_seq: string, disposition: InputDispositionDisposition, boundary_ref: string, };

export type InputDispositionDisposition = "applied" | "deferred" | "superseded" | "rejected";

export type Lease = { run_id: string, attempt_id: string, epoch: string, placement: Placement, authority: ExecutionAuthority, environment_id: string | null, generation: string | null, expires_at: string, boot_id: string, monotonic_deadline_ns: string, receipt_digest: string, };

export type Limits = { model_steps: number, active_wall_ms: number, output_bytes: number, };

export type NormalizedModelResponse = { model_call_id: string, step_id: string, items: Array<NormalizedModelResponseItemsItem>, finish: NormalizedModelResponseFinish, usage_refs: Array<string>, diagnostic_error: SafeError | null, };

export type NormalizedModelResponseFinish = "tools" | "final" | "length" | "refused" | "interrupted" | "failed";

export type NormalizedModelResponseItemsItem = { "kind": "text", text: string, } | { "kind": "tool_proposal", provider_call_id: string, binding_id: string, arguments: Record<string, unknown>, } | { "kind": "opaque_continuation", provider_profile_id: string, compatibility_digest: string, blob_ref: string, };

export type PendingReason = "unsent" | "unacknowledged" | "partitioned";

export type Placement = "local" | "cloud";

export type ProtocolVersion = "1.0";

export type RemoteOperationHandle = { installation_id: string, invocation_id: string, opaque_handle: string, expires_at: string, };

export type RequestFrameKind = "request";

export type RpcEvent = { "durability": "committed", frame_kind: RpcEventCommittedFrameKind, subscription_id: string, event: Event, } | { "durability": "provisional", frame_kind: RpcEventCommittedFrameKind, subscription_id: string, run_id: string, cursor: string, kind: RpcEventProvisionalKind, text: string, truncated: boolean, };

export type RpcEventCommittedFrameKind = "event";

export type RpcEventProvisionalKind = "model_text" | "stdout" | "stderr" | "gap";

export type RpcRequest = { "operation": "handshake", frame_kind: RequestFrameKind, request_id: string, body: RpcRequestHandshakeBody, } | { "operation": "describe_capabilities", frame_kind: RequestFrameKind, request_id: string, body: RpcRequestDescribeCapabilitiesBody, } | { "operation": "submit_task", frame_kind: RequestFrameKind, request_id: string, body: RpcRequestSubmitTaskBody, } | { "operation": "submit_command", frame_kind: RequestFrameKind, request_id: string, body: RpcRequestSubmitCommandBody, } | { "operation": "get_task_snapshot", frame_kind: RequestFrameKind, request_id: string, body: RpcRequestGetTaskSnapshotBody, } | { "operation": "subscribe", frame_kind: RequestFrameKind, request_id: string, body: RpcRequestSubscribeBody, } | { "operation": "answer_clarification", frame_kind: RequestFrameKind, request_id: string, body: RpcRequestAnswerClarificationBody, } | { "operation": "review_artifact", frame_kind: RequestFrameKind, request_id: string, body: RpcRequestReviewArtifactBody, } | { "operation": "decide_action", frame_kind: RequestFrameKind, request_id: string, body: RpcRequestDecideActionBody, } | { "operation": "get_artifact", frame_kind: RequestFrameKind, request_id: string, body: RpcRequestGetArtifactBody, } | { "operation": "prepare_transfer", frame_kind: RequestFrameKind, request_id: string, body: RpcRequestPrepareTransferBody, } | { "operation": "query_transfer", frame_kind: RequestFrameKind, request_id: string, body: RpcRequestQueryTransferBody, } | { "operation": "request_transfer_abort", frame_kind: RequestFrameKind, request_id: string, body: RpcRequestRequestTransferAbortBody, } | { "operation": "get_usage", frame_kind: RequestFrameKind, request_id: string, body: RpcRequestGetUsageBody, } | { "operation": "harness.describe_capabilities", frame_kind: RequestFrameKind, request_id: string, body: RpcRequestHarnessDescribeCapabilitiesBody, } | { "operation": "harness.start_turn", frame_kind: RequestFrameKind, request_id: string, body: RpcRequestHarnessStartTurnBody, } | { "operation": "harness.submit_input", frame_kind: RequestFrameKind, request_id: string, body: RpcRequestHarnessSubmitInputBody, } | { "operation": "harness.request_pause", frame_kind: RequestFrameKind, request_id: string, body: RpcRequestHarnessRequestPauseBody, } | { "operation": "harness.cancel", frame_kind: RequestFrameKind, request_id: string, body: RpcRequestHarnessCancelBody, } | { "operation": "harness.prepare_checkpoint", frame_kind: RequestFrameKind, request_id: string, body: RpcRequestHarnessPrepareCheckpointBody, } | { "operation": "harness.resume_turn", frame_kind: RequestFrameKind, request_id: string, body: RpcRequestHarnessResumeTurnBody, } | { "operation": "harness.subscribe", frame_kind: RequestFrameKind, request_id: string, body: RpcRequestHarnessSubscribeBody, };

export type RpcRequestAnswerClarificationBody = { command_id: string, run_id: string, idempotency_key: string, kind: RpcRequestAnswerClarificationBodyKind, payload: RpcRequestAnswerClarificationBodyPayload, };

export type RpcRequestAnswerClarificationBodyKind = "answer_clarification";

export type RpcRequestAnswerClarificationBodyPayload = { request_id: string, request_revision: string, answers: Array<RpcRequestAnswerClarificationBodyPayloadAnswersItem>, };

export type RpcRequestAnswerClarificationBodyPayloadAnswersItem = { question_id: string, answer: string, };

export type RpcRequestDecideActionBody = { command_id: string, run_id: string, idempotency_key: string, kind: RpcRequestDecideActionBodyKind, payload: RpcRequestDecideActionBodyPayload, };

export type RpcRequestDecideActionBodyKind = "decide_action";

export type RpcRequestDecideActionBodyPayload = { decision_id: string, decision_revision: string, invocation_id: string, target_digest: string, payload_digest: string, response: CommandDecideActionPayloadResponse, };

export type RpcRequestDescribeCapabilitiesBody = Record<symbol, never>;

export type RpcRequestGetArtifactBody = { artifact_id: string, version_id: string, cursor: string | null, max_bytes: number, };

export type RpcRequestGetTaskSnapshotBody = { run_id: string, };

export type RpcRequestGetUsageBody = { scope_id: string, starts_at: string, ends_at: string, };

export type RpcRequestHandshakeBody = { supported_versions: Array<string>, client_kind: RpcRequestHandshakeBodyClientKind, build_digest: string, nonce: string, challenge_response: string, requested_workspace_id: string | null, };

export type RpcRequestHandshakeBodyClientKind = "electron_main" | "supervisor" | "harness" | "guest_agent";

export type RpcRequestHarnessCancelBody = { run_id: string, attempt_id: string, cancel_command_id: string, };

export type RpcRequestHarnessDescribeCapabilitiesBody = Record<symbol, never>;

export type RpcRequestHarnessPrepareCheckpointBody = { run_id: string, attempt_id: string, event_cut: string, accepted_cut: string, coverage: FileCoverage, reason: RpcRequestHarnessPrepareCheckpointBodyReason, };

export type RpcRequestHarnessPrepareCheckpointBodyReason = "turn_limit" | "pause" | "transfer" | "restore" | "context_limit";

export type RpcRequestHarnessRequestPauseBody = { run_id: string, attempt_id: string, pause_command_id: string, };

export type RpcRequestHarnessResumeTurnBody = { run_spec: RunSpec, lease: Lease, turn_id: string, checkpoint_id: string, accepted_input_cut: string, limits: Limits, };

export type RpcRequestHarnessStartTurnBody = { run_spec: RunSpec, lease: Lease, turn_id: string, accepted_input_cut: string, checkpoint_id: string | null, limits: Limits, };

export type RpcRequestHarnessSubmitInputBody = { run_id: string, attempt_id: string, command_id: string, accepted_seq: string, payload_digest: string, };

export type RpcRequestHarnessSubscribeBody = { run_id: string, after_sequence: string, provisional_cursor: string | null, };

export type RpcRequestPrepareTransferBody = { run_id: string, expected_revision: string, target_profile_id: string, selected_manifest_id: string, idempotency_key: string, };

export type RpcRequestQueryTransferBody = { transfer_id: string, };

export type RpcRequestRequestTransferAbortBody = { transfer_id: string, idempotency_key: string, };

export type RpcRequestReviewArtifactBody = { command_id: string, run_id: string, idempotency_key: string, kind: RpcRequestReviewArtifactBodyKind, payload: RpcRequestReviewArtifactBodyPayload, };

export type RpcRequestReviewArtifactBodyKind = "review_artifact";

export type RpcRequestReviewArtifactBodyPayload = { request_id: string, request_revision: string, artifact_id: string, version_id: string, check_refs: Array<string>, response: CommandReviewArtifactPayloadResponse, };

export type RpcRequestSubmitCommandBody = { "kind": "steer", command_id: string, run_id: string, idempotency_key: string, payload: RpcRequestSubmitCommandBodySteerPayload, } | { "kind": "revise_objective", command_id: string, run_id: string, idempotency_key: string, payload: RpcRequestSubmitCommandBodyReviseObjectivePayload, } | { "kind": "pause", command_id: string, run_id: string, idempotency_key: string, payload: RpcRequestSubmitCommandBodyPausePayload, } | { "kind": "resume", command_id: string, run_id: string, idempotency_key: string, payload: RpcRequestSubmitCommandBodyResumePayload, } | { "kind": "stop", command_id: string, run_id: string, idempotency_key: string, payload: RpcRequestSubmitCommandBodyStopPayload, };

export type RpcRequestSubmitCommandBodyPausePayload = { expected_revision: string, reason: string, };

export type RpcRequestSubmitCommandBodyResumePayload = { expected_revision: string, checkpoint_id: string, };

export type RpcRequestSubmitCommandBodyReviseObjectivePayload = { objective_version_id: string, check_refs: Array<string>, expected_revision: string, };

export type RpcRequestSubmitCommandBodySteerPayload = { text: string, expected_revision: string, };

export type RpcRequestSubmitCommandBodyStopPayload = { reason: string, };

export type RpcRequestSubmitTaskBody = { objective: string, project_id: string | null, source_refs: Array<string>, check_template_ids: Array<string>, requested_placement: Placement, profile_id: string, lower_limits: Limits, idempotency_key: string, };

export type RpcRequestSubscribeBody = { run_id: string, after_sequence: string, provisional_cursor: string | null, };

export type RpcResponse = RpcResponseHandshakeOk | RpcResponseHandshakeError | RpcResponseDescribeCapabilitiesOk | RpcResponseDescribeCapabilitiesError | RpcResponseGetTaskSnapshotOk | RpcResponseGetTaskSnapshotError | RpcResponseSubscribeOk | RpcResponseSubscribeError | RpcResponseGetArtifactOk | RpcResponseGetArtifactError | RpcResponseGetUsageOk | RpcResponseGetUsageError | RpcResponseHarnessDescribeCapabilitiesOk | RpcResponseHarnessDescribeCapabilitiesError | RpcResponseHarnessStartTurnOk | RpcResponseHarnessStartTurnError | RpcResponseHarnessResumeTurnOk | RpcResponseHarnessResumeTurnError | RpcResponseHarnessPrepareCheckpointOk | RpcResponseHarnessPrepareCheckpointError | RpcResponseHarnessSubscribeOk | RpcResponseHarnessSubscribeError | RpcResponseHarnessRequestPauseOk | RpcResponseHarnessRequestPauseError | RpcResponseHarnessCancelOk | RpcResponseHarnessCancelError | RpcResponseHarnessSubmitInputOk | RpcResponseHarnessSubmitInputError | RpcResponseSubmitTaskOk | RpcResponseSubmitTaskError | RpcResponseSubmitCommandOk | RpcResponseSubmitCommandError | RpcResponseAnswerClarificationOk | RpcResponseAnswerClarificationError | RpcResponseReviewArtifactOk | RpcResponseReviewArtifactError | RpcResponseDecideActionOk | RpcResponseDecideActionError | RpcResponsePrepareTransferOk | RpcResponsePrepareTransferError | RpcResponseQueryTransferOk | RpcResponseQueryTransferError | RpcResponseRequestTransferAbortOk | RpcResponseRequestTransferAbortError;

export type RpcResponseAnswerClarificationError = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcRequestAnswerClarificationBodyKind, ok: false, error: SafeError, };

export type RpcResponseAnswerClarificationOk = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcRequestAnswerClarificationBodyKind, ok: true, result: CommandReceipt, };

export type RpcResponseDecideActionError = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcRequestDecideActionBodyKind, ok: false, error: SafeError, };

export type RpcResponseDecideActionOk = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcRequestDecideActionBodyKind, ok: true, result: CommandReceipt, };

export type RpcResponseDescribeCapabilitiesError = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseDescribeCapabilitiesOkOperation, ok: false, error: SafeError, };

export type RpcResponseDescribeCapabilitiesOk = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseDescribeCapabilitiesOkOperation, ok: true, result: CapabilityDescription, };

export type RpcResponseDescribeCapabilitiesOkOperation = "describe_capabilities";

export type RpcResponseGetArtifactError = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseGetArtifactOkOperation, ok: false, error: SafeError, };

export type RpcResponseGetArtifactOk = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseGetArtifactOkOperation, ok: true, result: ArtifactReadResult, };

export type RpcResponseGetArtifactOkOperation = "get_artifact";

export type RpcResponseGetTaskSnapshotError = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseGetTaskSnapshotOkOperation, ok: false, error: SafeError, };

export type RpcResponseGetTaskSnapshotOk = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseGetTaskSnapshotOkOperation, ok: true, result: TaskSnapshot, };

export type RpcResponseGetTaskSnapshotOkOperation = "get_task_snapshot";

export type RpcResponseGetUsageError = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseGetUsageOkOperation, ok: false, error: SafeError, };

export type RpcResponseGetUsageOk = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseGetUsageOkOperation, ok: true, result: UsageReport, };

export type RpcResponseGetUsageOkOperation = "get_usage";

export type RpcResponseHandshakeError = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseHandshakeOkOperation, ok: false, error: SafeError, };

export type RpcResponseHandshakeOk = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseHandshakeOkOperation, ok: true, result: RpcResponseHandshakeOkResult, };

export type RpcResponseHandshakeOkFrameKind = "response";

export type RpcResponseHandshakeOkOperation = "handshake";

export type RpcResponseHandshakeOkResult = { selected_version: ProtocolVersion, supervisor_generation: string, schema_min: string, schema_max: string, capability_manifest_digest: string, max_frame_bytes: 1048576, authenticated_scope_id: string, };

export type RpcResponseHarnessCancelError = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseHarnessCancelOkOperation, ok: false, error: SafeError, };

export type RpcResponseHarnessCancelOk = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseHarnessCancelOkOperation, ok: true, result: RpcResponseHarnessCancelOkResult, };

export type RpcResponseHarnessCancelOkOperation = "harness.cancel";

export type RpcResponseHarnessCancelOkResult = { "kind": "cancelled", checkpoint_id: string | null, cleanup_refs: Array<string>, unresolved_receipt_refs: Array<string>, } | { "kind": "failed", error: SafeError, retry_class: TurnOutcomeFailedRetryClass, checkpoint_id: string | null, };

export type RpcResponseHarnessDescribeCapabilitiesError = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseHarnessDescribeCapabilitiesOkOperation, ok: false, error: SafeError, };

export type RpcResponseHarnessDescribeCapabilitiesOk = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseHarnessDescribeCapabilitiesOkOperation, ok: true, result: CapabilityDescription, };

export type RpcResponseHarnessDescribeCapabilitiesOkOperation = "harness.describe_capabilities";

export type RpcResponseHarnessPrepareCheckpointError = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseHarnessPrepareCheckpointOkOperation, ok: false, error: SafeError, };

export type RpcResponseHarnessPrepareCheckpointOk = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseHarnessPrepareCheckpointOkOperation, ok: true, result: Checkpoint, };

export type RpcResponseHarnessPrepareCheckpointOkOperation = "harness.prepare_checkpoint";

export type RpcResponseHarnessRequestPauseError = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseHarnessRequestPauseOkOperation, ok: false, error: SafeError, };

export type RpcResponseHarnessRequestPauseOk = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseHarnessRequestPauseOkOperation, ok: true, result: RpcResponseHarnessRequestPauseOkResult, };

export type RpcResponseHarnessRequestPauseOkOperation = "harness.request_pause";

export type RpcResponseHarnessRequestPauseOkResult = { "kind": "paused", reason: TurnOutcomeYieldedReason, checkpoint_id: string, } | { "kind": "failed", error: SafeError, retry_class: TurnOutcomeFailedRetryClass, checkpoint_id: string | null, };

export type RpcResponseHarnessResumeTurnError = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseHarnessResumeTurnOkOperation, ok: false, error: SafeError, };

export type RpcResponseHarnessResumeTurnOk = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseHarnessResumeTurnOkOperation, ok: true, result: RpcResponseHarnessResumeTurnOkResult, };

export type RpcResponseHarnessResumeTurnOkOperation = "harness.resume_turn";

export type RpcResponseHarnessResumeTurnOkResult = { turn_id: string, accepted: true, };

export type RpcResponseHarnessStartTurnError = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseHarnessStartTurnOkOperation, ok: false, error: SafeError, };

export type RpcResponseHarnessStartTurnOk = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseHarnessStartTurnOkOperation, ok: true, result: RpcResponseHarnessStartTurnOkResult, };

export type RpcResponseHarnessStartTurnOkOperation = "harness.start_turn";

export type RpcResponseHarnessStartTurnOkResult = { turn_id: string, accepted: true, };

export type RpcResponseHarnessSubmitInputError = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseHarnessSubmitInputOkOperation, ok: false, error: SafeError, };

export type RpcResponseHarnessSubmitInputOk = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseHarnessSubmitInputOkOperation, ok: true, result: InputDisposition, };

export type RpcResponseHarnessSubmitInputOkOperation = "harness.submit_input";

export type RpcResponseHarnessSubscribeError = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseHarnessSubscribeOkOperation, ok: false, error: SafeError, };

export type RpcResponseHarnessSubscribeOk = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseHarnessSubscribeOkOperation, ok: true, result: Subscription, };

export type RpcResponseHarnessSubscribeOkOperation = "harness.subscribe";

export type RpcResponsePrepareTransferError = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponsePrepareTransferOkOperation, ok: false, error: SafeError, };

export type RpcResponsePrepareTransferOk = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponsePrepareTransferOkOperation, ok: true, result: TransferReceipt, };

export type RpcResponsePrepareTransferOkOperation = "prepare_transfer";

export type RpcResponseQueryTransferError = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseQueryTransferOkOperation, ok: false, error: SafeError, };

export type RpcResponseQueryTransferOk = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseQueryTransferOkOperation, ok: true, result: TransferReceipt, };

export type RpcResponseQueryTransferOkOperation = "query_transfer";

export type RpcResponseRequestTransferAbortError = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseRequestTransferAbortOkOperation, ok: false, error: SafeError, };

export type RpcResponseRequestTransferAbortOk = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseRequestTransferAbortOkOperation, ok: true, result: TransferReceipt, };

export type RpcResponseRequestTransferAbortOkOperation = "request_transfer_abort";

export type RpcResponseReviewArtifactError = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcRequestReviewArtifactBodyKind, ok: false, error: SafeError, };

export type RpcResponseReviewArtifactOk = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcRequestReviewArtifactBodyKind, ok: true, result: CommandReceipt, };

export type RpcResponseSubmitCommandError = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseSubmitCommandOkOperation, ok: false, error: SafeError, };

export type RpcResponseSubmitCommandOk = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseSubmitCommandOkOperation, ok: true, result: CommandReceipt, };

export type RpcResponseSubmitCommandOkOperation = "submit_command";

export type RpcResponseSubmitTaskError = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseSubmitTaskOkOperation, ok: false, error: SafeError, };

export type RpcResponseSubmitTaskOk = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseSubmitTaskOkOperation, ok: true, result: CommandReceipt, };

export type RpcResponseSubmitTaskOkOperation = "submit_task";

export type RpcResponseSubscribeError = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseSubscribeOkOperation, ok: false, error: SafeError, };

export type RpcResponseSubscribeOk = { frame_kind: RpcResponseHandshakeOkFrameKind, request_id: string, operation: RpcResponseSubscribeOkOperation, ok: true, result: Subscription, };

export type RpcResponseSubscribeOkOperation = "subscribe";

export type RunSpec = { task_id: string, task_revision_id: string, run_id: string, session_id: string, branch_id: string, workspace_id: string, project_id: string | null, worker_id: string | null, responsibility_id: string | null, requester_id: string, owner_id: string, acting_principal_id: string, audience_policy_id: string, execution_policy_id: string, objective_version_id: string, check_refs: Array<string>, instruction_digests: Array<string>, model_profile_id: string, environment_recipe_version_id: string | null, budget_period_refs: Array<string>, deadline: string | null, placement: Placement, accepted_input_seq: string, };

export type RunState = "queued" | "preparing" | "running" | "waiting" | "paused" | "blocked" | "transferring" | "verifying" | "recovering" | "cancelling" | "completed" | "cancelled" | "failed";

export type SafeError = { code: ErrorCode, message: string, evidence_refs: Array<string>, };

export type StepBinding = { step_id: string, model_profile_id: string, model_adapter_digest: string, prompt_schema_version: ProtocolVersion, catalog_digest: string, tool_binding_ids: Array<string>, instruction_digests: Array<string>, context_manifest_id: string, environment_capability_digest: string, permission_generation: string, deletion_generation: string, sealed_at: string, };

export type Subscription = { subscription_id: string, snapshot: TaskSnapshot, replay_after: string, resync_required: boolean, };

export type TaskSnapshot = { task_id: string, task_revision_id: string, run_id: string, row_version: string, state: RunState, authority: Authority, placement: Placement, accepted_seq: string, applied_seq: string, last_event_seq: string, cancellation_requested: boolean, artifact_refs: Array<ArtifactRef>, check_refs: Array<string>, receipt_refs: Array<string>, wait_refs: Array<string>, coverage_manifest_ref: string | null, };

export type TerminalRunState = "completed" | "cancelled" | "failed";

export type ToolResult = { "status": "succeeded", invocation_id: string, schema_version: ProtocolVersion, artifact_refs: Array<ArtifactRef>, receipt_ref: string | null, truncated: boolean, next_cursor: string | null, usage_refs: Array<string>, data: BoundData, } | { "status": "running", invocation_id: string, schema_version: ProtocolVersion, artifact_refs: Array<ArtifactRef>, receipt_ref: string | null, truncated: boolean, next_cursor: string | null, usage_refs: Array<string>, data: ToolResultRunningData, } | { "status": "awaiting_input", invocation_id: string, schema_version: ProtocolVersion, artifact_refs: Array<ArtifactRef>, receipt_ref: string | null, truncated: boolean, next_cursor: string | null, usage_refs: Array<string>, data: ToolResultAwaitingInputData, } | { "status": "denied", invocation_id: string, schema_version: ProtocolVersion, artifact_refs: Array<ArtifactRef>, receipt_ref: string | null, truncated: boolean, next_cursor: string | null, usage_refs: Array<string>, error: SafeError, data: BoundData | null, } | { "status": "conflict", invocation_id: string, schema_version: ProtocolVersion, artifact_refs: Array<ArtifactRef>, receipt_ref: string | null, truncated: boolean, next_cursor: string | null, usage_refs: Array<string>, error: SafeError, data: BoundData | null, } | { "status": "timed_out", invocation_id: string, schema_version: ProtocolVersion, artifact_refs: Array<ArtifactRef>, receipt_ref: string | null, truncated: boolean, next_cursor: string | null, usage_refs: Array<string>, error: SafeError, data: BoundData | null, } | { "status": "cancelled", invocation_id: string, schema_version: ProtocolVersion, artifact_refs: Array<ArtifactRef>, receipt_ref: string | null, truncated: boolean, next_cursor: string | null, usage_refs: Array<string>, error: SafeError, data: BoundData | null, } | { "status": "lost", invocation_id: string, schema_version: ProtocolVersion, artifact_refs: Array<ArtifactRef>, receipt_ref: string | null, truncated: boolean, next_cursor: string | null, usage_refs: Array<string>, error: SafeError, data: BoundData | null, } | { "status": "unknown", invocation_id: string, schema_version: ProtocolVersion, artifact_refs: Array<ArtifactRef>, receipt_ref: string | null, truncated: boolean, next_cursor: string | null, usage_refs: Array<string>, error: SafeError, data: BoundData | null, } | { "status": "failed", invocation_id: string, schema_version: ProtocolVersion, artifact_refs: Array<ArtifactRef>, receipt_ref: string | null, truncated: boolean, next_cursor: string | null, usage_refs: Array<string>, error: SafeError, data: BoundData | null, };

export type ToolResultAwaitingInputData = { request_id: string, wait_id: string, request_kind: ToolResultAwaitingInputDataRequestKind, };

export type ToolResultAwaitingInputDataRequestKind = "clarification" | "action_approval" | "artifact_review";

export type ToolResultRunningData = ToolResultRunningDataV1 | ToolResultRunningDataV2;

export type ToolResultRunningDataV1 = { handle: ToolResultRunningDataV1Handle, stdout_cursor: string | null, stderr_cursor: string | null, };

export type ToolResultRunningDataV1Handle = { environment_id: string, generation: string, handle: string, };

export type ToolResultRunningDataV2 = { operation_handle: RemoteOperationHandle, poll_after_ms: number, };

export type TransferDisposition = "preflight" | "pending" | "committed" | "aborted" | "blocked";

export type TransferReceipt = { transfer_id: string, run_id: string, disposition: TransferDisposition, authority: Authority, source_epoch: string, destination_epoch: string | null, accepted_seq: string, receipt_ref: string | null, gap_codes: Array<string>, };

export type TrustedContext = { workspace_id: string, project_id: string | null, run_id: string, attempt_id: string, step_id: string, principal_id: string, epoch: string, environment_id: string | null, generation: string | null, binding_id: string, catalog_digest: string, budget_hold_refs: Array<string>, approval_ref: string | null, };

export type TurnOutcome = { "kind": "candidate_result", objective_version_id: string, artifact_refs: Array<ArtifactRef>, check_result_refs: Array<string>, checkpoint_id: string, } | { "kind": "wait_requested", wait: WaitSpec, checkpoint_id: string, } | { "kind": "yielded", reason: TurnOutcomeYieldedReason, checkpoint_id: string, } | { "kind": "paused", reason: TurnOutcomeYieldedReason, checkpoint_id: string, } | { "kind": "cancelled", checkpoint_id: string | null, cleanup_refs: Array<string>, unresolved_receipt_refs: Array<string>, } | { "kind": "failed", error: SafeError, retry_class: TurnOutcomeFailedRetryClass, checkpoint_id: string | null, };

export type TurnOutcomeFailedRetryClass = "safe_read" | "reconcile_first" | "remediation" | "terminal";

export type TurnOutcomeYieldedReason = "turn_limit" | "context_limit" | "user" | "budget" | "resource_limit";

export type UsageReport = { scope_id: string, currency: UsageReportCurrency, segments: Array<UsageReportSegmentsItem>, };

export type UsageReportCurrency = "USD";

export type UsageReportSegmentsItem = { kind: UsageReportSegmentsItemKind, settled_amount: string, estimated_amount: string | null, unsettled_refs: Array<string>, budget_period_refs: Array<string>, };

export type UsageReportSegmentsItemKind = "local" | "cloud_agent" | "worker";

export type WaitSpec = { wait_id: string, mode: WaitSpecMode, conditions: Array<WaitSpecConditionsItem>, watermark: string, deadline: string | null, };

export type WaitSpecConditionsItem = { "kind": "human", request_id: string, request_revision: string, } | { "kind": "timer", due_at: string, } | { "kind": "event", resource_id: string, expected_version_id: string, event_kind: string, after_watermark: string, } | { "kind": "process", handle: WaitSpecConditionsItemProcessHandle, } | { "kind": "child", child_run_id: string, required_output_schema_digest: string, } | { "kind": "budget", budget_period_id: string, };

export type WaitSpecConditionsItemProcessHandle = { environment_id: string, generation: string, handle: string, };

export type WaitSpecMode = "all" | "any";

export type WireMessage = { "message_kind": "command", schema_version: ProtocolVersion, payload: Command, } | { "message_kind": "command_receipt", schema_version: ProtocolVersion, payload: CommandReceipt, } | { "message_kind": "lease", schema_version: ProtocolVersion, payload: Lease, } | { "message_kind": "step_binding", schema_version: ProtocolVersion, payload: StepBinding, } | { "message_kind": "trusted_context", schema_version: ProtocolVersion, payload: TrustedContext, } | { "message_kind": "run_spec", schema_version: ProtocolVersion, payload: RunSpec, } | { "message_kind": "event", schema_version: ProtocolVersion, payload: Event, } | { "message_kind": "tool_result", schema_version: ProtocolVersion, payload: ToolResult, } | { "message_kind": "wait_spec", schema_version: ProtocolVersion, payload: WaitSpec, } | { "message_kind": "checkpoint", schema_version: ProtocolVersion, payload: Checkpoint, } | { "message_kind": "turn_outcome", schema_version: ProtocolVersion, payload: TurnOutcome, } | { "message_kind": "rpc_request", schema_version: ProtocolVersion, payload: RpcRequest, } | { "message_kind": "rpc_response", schema_version: ProtocolVersion, payload: RpcResponse, } | { "message_kind": "rpc_event", schema_version: ProtocolVersion, payload: RpcEvent, } | { "message_kind": "normalized_model_response", schema_version: ProtocolVersion, payload: NormalizedModelResponse, } | { "message_kind": "context_manifest", schema_version: ProtocolVersion, payload: ContextManifest, };
