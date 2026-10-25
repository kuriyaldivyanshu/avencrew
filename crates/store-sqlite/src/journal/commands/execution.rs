//! Authoritative bounded fixture execution journal. No inference or tool IO here.
use super::checkpoints::digest_bytes;
use super::*;
use crate::journal::blobs::hex;
use avencrew_contracts::{
    runtime::{FixtureUsage, LocalStep, LocalTurnProgress, ModelCompletion, TurnState},
    wire::{ExecutionAuthority, NormalizedModelResponseFinish, Placement},
};

const STEP_MEDIA: &str = "application/vnd.avencrew.sealed-step+json";
const MODEL_MEDIA: &str = "application/vnd.avencrew.model-completion+json";
const PROGRESS_MEDIA: &str = "application/vnd.avencrew.turn-progress+json";
const CATALOG_MEDIA: &str = "application/vnd.avencrew.sealed-catalog+json";
const PROFILE_MEDIA: &str = "application/vnd.avencrew.model-profile+json";
mod invocations;
pub use invocations::LocalDispatch;
mod controls;
mod recovery;
pub use controls::LocalControlSnapshot;
pub use recovery::LocalExecutionRestore;
mod artifacts;
mod checkpoints;
pub use artifacts::{
    LocalArtifact, LocalArtifactPublication, LocalArtifactReceipt, LocalArtifactTarget,
};
pub use checkpoints::LocalExecutionCheckpoint;
mod budget;
mod context;
pub use budget::LocalExecutionBudget;

/// Protected coordinator observations, not client-supplied permission fields.
pub struct LocalExecutionAccess<'a> {
    pub workspace: &'a DomainId,
    pub actor: &'a DomainId,
    pub turn: &'a DomainId,
    pub now: &'a Instant,
    pub boot: &'a DomainId,
    pub monotonic_now: &'a Counter,
    sampled_at: std::time::Instant,
}
impl<'a> LocalExecutionAccess<'a> {
    pub fn new(
        workspace: &'a DomainId,
        actor: &'a DomainId,
        turn: &'a DomainId,
        now: &'a Instant,
        boot: &'a DomainId,
        monotonic_now: &'a Counter,
    ) -> Self {
        Self {
            workspace,
            actor,
            turn,
            now,
            boot,
            monotonic_now,
            sampled_at: std::time::Instant::now(),
        }
    }
}

pub struct LocalExecutionSnapshot {
    pub version: u64,
    pub progress: LocalTurnProgress,
    pub step: Option<LocalStep>,
    pub model: Option<ModelCompletion>,
    pub current_checkpoint: Option<DomainId>,
    pub cancellation_requested: bool,
    pub unresolved_invocations: Vec<DomainId>,
    pub unresolved_model_calls: Vec<DomainId>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Disposition {
    Applied,
    Superseded,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InputDisposition {
    command_id: DomainId,
    sequence: PositiveCounter,
    disposition: Disposition,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProgressRecord {
    schema_version: u32,
    turn_id: DomainId,
    version: u64,
    progress: LocalTurnProgress,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    input_dispositions: Vec<InputDisposition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    observed_invocation_ids: Vec<DomainId>,
}

impl LocalStore {
    /// Historical projection only. Restoring this snapshot never renews a lease.
    pub async fn local_execution_snapshot(
        &mut self,
        workspace: &DomainId,
        actor: &DomainId,
        turn: &DomainId,
    ) -> Result<LocalExecutionSnapshot, TurnError> {
        let retained = self.read_local_turn(workspace, actor, turn).await?;
        let run = run_row(
            &mut self.connection,
            workspace,
            &retained.seed.run_spec.run_id,
            actor,
        )
        .await?;
        let row = sqlx::query(
            "SELECT runtime_version,runtime_blob_id FROM local_turns WHERE workspace_id=? AND id=?",
        )
        .bind(workspace.as_str())
        .bind(turn.as_str())
        .fetch_one(&mut self.connection)
        .await?;
        let version: i64 = row.try_get("runtime_version")?;
        let pointer: Option<String> = row.try_get("runtime_blob_id")?;
        let progress = if version == 0 {
            if pointer.is_some() {
                return Err(TurnError::Conflict);
            }
            LocalTurnProgress {
                state: TurnState {
                    steps_started: 0,
                    no_progress_steps: 0,
                    active_elapsed_ms: 0,
                    applied_input_cut: Counter::new(run.applied.to_string())
                        .map_err(|_| TurnError::Conflict)?,
                    progress_digest: self
                        .execution_progress_digest(workspace, actor, &retained.seed)
                        .await?,
                },
                pending_step_id: None,
                checkpoint_id: None,
                outcome: None,
            }
        } else {
            let id = DomainId::new(pointer.ok_or(TurnError::Conflict)?)
                .map_err(|_| TurnError::Conflict)?;
            let bytes = self
                .read_registered_blob(workspace, actor, &id, PROGRESS_MEDIA)
                .await?;
            let record: ProgressRecord =
                serde_json::from_slice(&bytes).map_err(|_| TurnError::Conflict)?;
            if canonical(&record)? != bytes
                || record.schema_version != 1
                || record.turn_id != *turn
                || record.version != version as u64
            {
                return Err(TurnError::Conflict);
            }
            let present: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM local_events WHERE workspace_id=? AND run_id=? AND id=? AND payload_blob_id=? AND attempt_id=? AND epoch=? AND event_type LIKE 'execution.%')")
                .bind(workspace.as_str()).bind(retained.seed.run_spec.run_id.as_str()).bind(id.as_str()).bind(id.as_str())
                .bind(retained.seed.lease.attempt_id.as_str()).bind(retained.seed.lease.epoch.value() as i64).fetch_one(&mut self.connection).await?;
            if !present {
                return Err(TurnError::Conflict);
            }
            record.progress
        };
        if progress.state.steps_started as u64 > retained.seed.limits.model_steps.value()
            || progress.state.no_progress_steps > 50
            || progress.state.applied_input_cut.value() > run.accepted as u64
            || (progress.outcome.is_some() && progress.pending_step_id.is_some())
        {
            return Err(TurnError::Conflict);
        }
        let mut model = None;
        let step = if let Some(id) = &progress.pending_step_id {
            let step = self.read_local_step(workspace, actor, turn, id).await?;
            model = self
                .read_local_model_completion(workspace, actor, &step)
                .await?;
            Some(step)
        } else {
            None
        };
        let unresolved:Vec<String>=sqlx::query_scalar("SELECT id FROM local_invocations WHERE workspace_id=? AND run_id=? AND state IN ('dispatched','unknown') ORDER BY id LIMIT 101")
            .bind(workspace.as_str()).bind(retained.seed.run_spec.run_id.as_str()).fetch_all(&mut self.connection).await?;
        if unresolved.len() > 100 {
            return Err(TurnError::Conflict);
        }
        let models:Vec<String>=sqlx::query_scalar("SELECT c.id FROM local_model_calls c JOIN local_steps s ON s.workspace_id=c.workspace_id AND s.id=c.step_id WHERE s.workspace_id=? AND s.turn_id=? AND c.state='intent' ORDER BY c.id LIMIT 51")
            .bind(workspace.as_str()).bind(turn.as_str()).fetch_all(&mut self.connection).await?;
        // TurnOutcome::Cancelled and Control::Stop share a wire cap of 100.
        // Do not admit a projection that cannot retain its exact unresolved set.
        if models.len() > 50 || unresolved.len() + models.len() > 100 {
            return Err(TurnError::Conflict);
        }
        Ok(LocalExecutionSnapshot {
            version: version as u64,
            progress,
            step,
            model,
            cancellation_requested: run.cancelled,
            unresolved_model_calls: models
                .into_iter()
                .map(DomainId::new)
                .collect::<Result<_, _>>()
                .map_err(|_| TurnError::Conflict)?,
            unresolved_invocations: unresolved
                .into_iter()
                .map(DomainId::new)
                .collect::<Result<_, _>>()
                .map_err(|_| TurnError::Conflict)?,
            current_checkpoint: sqlx::query_scalar::<_, Option<String>>(
                "SELECT current_checkpoint_id FROM local_runs WHERE workspace_id=? AND id=?",
            )
            .bind(workspace.as_str())
            .bind(retained.seed.run_spec.run_id.as_str())
            .fetch_one(&mut self.connection)
            .await?
            .map(DomainId::new)
            .transpose()
            .map_err(|_| TurnError::Conflict)?,
        })
    }

    async fn read_local_model_completion(
        &mut self,
        w: &DomainId,
        actor: &DomainId,
        step: &LocalStep,
    ) -> Result<Option<ModelCompletion>, TurnError> {
        let row=sqlx::query("SELECT * FROM local_model_calls WHERE workspace_id=? AND id=? AND step_id=? AND ordinal=1").bind(w.as_str()).bind(step.model_call_id.as_str()).bind(step.binding.step_id.as_str()).fetch_one(&mut self.connection).await?;
        if row.try_get::<Vec<u8>, _>("request_digest")?
            != digest_bytes(step.request_digest.as_str())
        {
            return Err(TurnError::Conflict);
        }
        let pointer: Option<String> = row.try_get("response_blob_id")?;
        let Some(pointer) = pointer else {
            if row.try_get::<String, _>("state")? != "intent"
                || row.try_get::<Option<i64>, _>("input_tokens")?.is_some()
                || row.try_get::<Option<i64>, _>("output_tokens")?.is_some()
                || row.try_get::<Option<i64>, _>("ended_at_us")?.is_some()
            {
                return Err(TurnError::Conflict);
            }
            return Ok(None);
        };
        let blob = DomainId::new(pointer).map_err(|_| TurnError::Conflict)?;
        let bytes = self
            .read_registered_blob(w, actor, &blob, MODEL_MEDIA)
            .await?;
        let result: ModelCompletion =
            serde_json::from_slice(&bytes).map_err(|_| TurnError::Conflict)?;
        if avencrew_contracts::runtime::record_bytes(&result).map_err(|_| TurnError::Conflict)?
            != bytes
        {
            return Err(TurnError::Conflict);
        }
        validate_completion(step, &result)?;
        let (input, output) = usage(&result);
        let error = result.as_ref().err().and_then(|f| {
            serde_json::to_value(f.code)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
        });
        if row.try_get::<Option<i64>, _>("input_tokens")? != input
            || row.try_get::<Option<i64>, _>("output_tokens")? != output
            || row.try_get::<Option<String>, _>("error_code")? != error
            || row.try_get::<String, _>("state")?
                != if result.is_ok() {
                    "response"
                } else {
                    "unknown"
                }
        {
            return Err(TurnError::Conflict);
        }
        Ok(Some(result))
    }

    pub async fn read_local_step(
        &mut self,
        workspace: &DomainId,
        actor: &DomainId,
        turn: &DomainId,
        step: &DomainId,
    ) -> Result<LocalStep, TurnError> {
        let _proof_scope = self.proof_read_scope();
        self.read_local_turn(workspace, actor, turn).await?;
        let row = sqlx::query("SELECT context_blob_id,tool_catalog_blob_id,model_profile_blob_id,run_id,attempt_id,sealed_at_us FROM local_steps WHERE workspace_id=? AND turn_id=? AND id=?")
            .bind(workspace.as_str()).bind(turn.as_str()).bind(step.as_str()).fetch_one(&mut self.connection).await?;
        let id = DomainId::new(row.try_get::<String, _>("context_blob_id")?)
            .map_err(|_| TurnError::Conflict)?;
        let bytes = self
            .read_registered_blob(workspace, actor, &id, STEP_MEDIA)
            .await?;
        let record: LocalStep = serde_json::from_slice(&bytes).map_err(|_| TurnError::Conflict)?;
        if (record.request["schema_version"] == "avencrew.fixture-context/2")
            != record.source_proof.is_some()
        {
            return Err(TurnError::Conflict);
        }
        if record.retained_bytes().map_err(|_| TurnError::Conflict)? != bytes
            || record.binding.step_id != *step
            || record.binding.context_manifest_id != id
            || record.context.workspace_id != *workspace
            || record.context.run_id.as_str() != row.try_get::<String, _>("run_id")?
            || record.context.attempt_id.as_str() != row.try_get::<String, _>("attempt_id")?
            || record.binding.sealed_at.unix_micros() != row.try_get::<i64, _>("sealed_at_us")?
        {
            return Err(TurnError::Conflict);
        }
        for (column, media, expected) in [
            (
                "tool_catalog_blob_id",
                CATALOG_MEDIA,
                canonical(&record.bindings)?,
            ),
            (
                "model_profile_blob_id",
                PROFILE_MEDIA,
                canonical(
                    &serde_json::json!({"profile_id":record.provider_profile_id,"compatibility_digest":record.compatibility_digest,"adapter_digest":record.binding.model_adapter_digest,"provider":"fixture","rate_micro":"0"}),
                )?,
            ),
        ] {
            let id = DomainId::new(row.try_get::<String, _>(column)?)
                .map_err(|_| TurnError::Conflict)?;
            if self
                .read_registered_blob(workspace, actor, &id, media)
                .await?
                != expected
            {
                return Err(TurnError::Conflict);
            }
        }
        if hex(&digest(
            &record.request_bytes().map_err(|_| TurnError::Conflict)?,
        )) != record.request_digest.as_str()
        {
            return Err(TurnError::Conflict);
        }
        self.verify_context_proof(workspace, actor, turn, &record)
            .await?;
        Ok(record)
    }

    /// Atomic seal + model intent + exact counters. A retry never returns fresh.
    pub async fn seal_local_step(
        &mut self,
        access: LocalExecutionAccess<'_>,
        step: &LocalStep,
        state: &TurnState,
        event: &DomainId,
    ) -> Result<bool, TurnError> {
        let seed = self.authorize_local_execution(&access, false).await?;
        if (step.request["schema_version"] == "avencrew.fixture-context/2")
            != step.source_proof.is_some()
        {
            return Err(TurnError::InvalidSeed);
        }
        let snapshot = self
            .local_execution_snapshot(access.workspace, access.actor, access.turn)
            .await?;
        if let Some(old) = &snapshot.step {
            if old != step {
                return Err(TurnError::IdempotencyMismatch);
            }
            return Ok(false);
        }
        if snapshot.unresolved_invocations.len() + snapshot.unresolved_model_calls.len() >= 100 {
            return Err(TurnError::Conflict);
        }
        step.binding
            .validate()
            .map_err(|_| TurnError::InvalidSeed)?;
        step.context
            .validate()
            .map_err(|_| TurnError::InvalidSeed)?;
        if step.accepted_input_cut != state.applied_input_cut
            || snapshot.progress.outcome.is_some()
            || state.steps_started != snapshot.progress.state.steps_started + 1
            || state.steps_started as u64 > seed.limits.model_steps.value()
            || state.applied_input_cut != snapshot.progress.state.applied_input_cut
            || step.lease != seed.lease
            || step.context.run_id != seed.run_spec.run_id
            || step.context.attempt_id != seed.lease.attempt_id
            || step.context.principal_id != seed.run_spec.acting_principal_id
            || step.context.project_id != seed.run_spec.project_id
            || step.context.workspace_id != *access.workspace
            || step.context.epoch != seed.lease.epoch
            || step.context.step_id != step.binding.step_id
            || step.context.environment_id != seed.lease.environment_id
            || step.context.generation != seed.lease.generation
            || step.binding.model_profile_id != seed.run_spec.model_profile_id
            || step.provider_profile_id != seed.run_spec.model_profile_id
            || step.binding.permission_generation.value() != 1
            || step.binding.deletion_generation.value() != 0
            || step.binding.sealed_at > *access.now
            || step.bindings.len() > 100
            || state.active_elapsed_ms > seed.limits.active_wall_ms.value()
            || state.no_progress_steps != snapshot.progress.state.no_progress_steps
            || state.progress_digest != snapshot.progress.state.progress_digest
        {
            return Err(TurnError::InvalidSeed);
        }
        let request = step.request_bytes().map_err(|_| TurnError::InvalidSeed)?;
        if request.len() + seed.limits.output_bytes.value() as usize > 1_048_576
            || hex(&digest(&request)) != step.request_digest.as_str()
        {
            return Err(TurnError::InvalidSeed);
        }
        let catalog = canonical(&step.bindings)?;
        let mut catalog_hash = b"avencrew.sealed-catalog/1\0".to_vec();
        catalog_hash.extend(&catalog);
        if hex(&digest(&catalog_hash)) != step.binding.catalog_digest.as_str()
            || step.context.catalog_digest != step.binding.catalog_digest
            || step.binding.tool_binding_ids.as_slice()
                != step
                    .bindings
                    .iter()
                    .map(|b| &b.binding_id)
                    .cloned()
                    .collect::<Vec<_>>()
        {
            return Err(TurnError::InvalidSeed);
        }
        let proof = if let Some(reference) = &step.source_proof {
            let budget = step
                .request
                .get("source")
                .and_then(|_| step.request.get("schema_version"));
            if budget != Some(&serde_json::json!("avencrew.fixture-context/2")) {
                return Err(TurnError::InvalidSeed);
            }
            // The configured budget is sealed into the proof and model view.
            let context_bytes = step.request["context_bytes"]
                .as_u64()
                .ok_or(TurnError::InvalidSeed)?;
            let proof = self
                .local_context_proof(
                    access.workspace,
                    access.actor,
                    access.turn,
                    u32::try_from(context_bytes).map_err(|_| TurnError::InvalidSeed)?,
                )
                .await?;
            let bytes = avencrew_contracts::runtime::record_bytes(&proof)
                .map_err(|_| TurnError::InvalidSeed)?;
            if context::proof_reference(reference.id.clone(), &bytes)? != *reference
                || avencrew_contracts::runtime::context::fixture_context_view(
                    &proof,
                    reference,
                    &step.compatibility_digest,
                )
                .map_err(|_| TurnError::InvalidSeed)?
                    != step.request
                || request.len() as u64 + seed.limits.output_bytes.value() > context_bytes
            {
                return Err(TurnError::InvalidSeed);
            }
            Some((reference.id.clone(), bytes, context::PROOF_MEDIA))
        } else {
            None
        };
        let bytes = step.retained_bytes().map_err(|_| TurnError::InvalidSeed)?;
        let profile = canonical(
            &serde_json::json!({"profile_id":step.provider_profile_id,"compatibility_digest":step.compatibility_digest,"adapter_digest":step.binding.model_adapter_digest,"provider":"fixture","rate_micro":"0"}),
        )?;
        let mut blobs = vec![
            (&step.binding.context_manifest_id, bytes, STEP_MEDIA),
            (&step.binding.step_id, catalog, CATALOG_MEDIA),
            (&step.model_call_id, profile, PROFILE_MEDIA),
        ];
        if let Some((id, bytes, media)) = &proof {
            blobs.push((id, bytes.clone(), *media));
        }
        for (id, bytes, _) in &blobs {
            self.put_canonical(access.workspace, id, bytes)?;
        }
        let progress = LocalTurnProgress {
            state: state.clone(),
            pending_step_id: Some(step.binding.step_id.clone()),
            checkpoint_id: snapshot.progress.checkpoint_id,
            outcome: None,
        };
        let progress_bytes = progress_bytes(access.turn, snapshot.version + 1, &progress)?;
        self.put_canonical(access.workspace, event, &progress_bytes)?;
        let identity = self
            .verified_identity(access.workspace, access.actor)
            .await?;
        let mut tx = self.connection.begin().await?;
        identity.check_live(&mut tx).await?;
        check_dispatch_cut(&mut tx, &access, &seed).await?;
        for (id, bytes, media) in &blobs {
            blob(
                &mut tx,
                access.workspace,
                access.actor,
                id,
                bytes,
                media,
                access.now.unix_micros(),
            )
            .await?;
        }
        sqlx::query("INSERT INTO local_steps (id,workspace_id,created_at_us,run_id,attempt_id,turn_id,turn_ordinal,step_ordinal,context_blob_id,tool_catalog_blob_id,model_profile_blob_id,sealed_at_us,state) VALUES (?,?,?,?,?,?,1,?,?,?,?,?,'sealed')")
            .bind(step.binding.step_id.as_str()).bind(access.workspace.as_str()).bind(access.now.unix_micros()).bind(seed.run_spec.run_id.as_str()).bind(seed.lease.attempt_id.as_str()).bind(access.turn.as_str()).bind(state.steps_started as i64)
            .bind(step.binding.context_manifest_id.as_str()).bind(step.binding.step_id.as_str()).bind(step.model_call_id.as_str()).bind(step.binding.sealed_at.unix_micros()).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO local_model_calls (id,workspace_id,created_at_us,step_id,ordinal,request_digest,state,started_at_us) VALUES (?,?,?,?,1,?,'intent',?)")
            .bind(step.model_call_id.as_str()).bind(access.workspace.as_str()).bind(access.now.unix_micros()).bind(step.binding.step_id.as_str()).bind(digest_bytes(step.request_digest.as_str())).bind(access.now.unix_micros()).execute(&mut *tx).await?;
        budget::record_usage(
            &mut tx,
            &access,
            &seed,
            budget::Measurement {
                id: event,
                invocation: None,
                meter: "model_call",
                source: &step.model_call_id,
                unit: "call",
                quantity: Some(1),
                price: &step.model_call_id,
            },
        )
        .await?;
        save_progress(
            &mut tx,
            &access,
            &seed,
            snapshot.version,
            event,
            "execution.model_intent",
            &progress_bytes,
        )
        .await?;
        #[cfg(test)]
        tests::barrier(&self.status.database_path, "intent_before_commit")?;
        tx.commit().await?;
        #[cfg(test)]
        tests::barrier(&self.status.database_path, "intent_after_commit")?;
        Ok(true)
    }

    pub async fn record_local_model(
        &mut self,
        access: LocalExecutionAccess<'_>,
        step: &LocalStep,
        result: &ModelCompletion,
        state: &TurnState,
        response_id: &DomainId,
        event: &DomainId,
    ) -> Result<(), TurnError> {
        let seed = self.authorize_local_execution(&access, true).await?;
        let snapshot = self
            .local_execution_snapshot(access.workspace, access.actor, access.turn)
            .await?;
        if snapshot.step.as_ref() != Some(step) {
            return Err(TurnError::Conflict);
        }
        if let Some(old) = snapshot.model {
            return if old == *result {
                Ok(())
            } else {
                Err(TurnError::IdempotencyMismatch)
            };
        }
        validate_completion(step, result)?;
        if state.steps_started != snapshot.progress.state.steps_started
            || state.applied_input_cut != snapshot.progress.state.applied_input_cut
            || state.active_elapsed_ms < snapshot.progress.state.active_elapsed_ms
            || state.no_progress_steps != snapshot.progress.state.no_progress_steps
            || state.progress_digest != snapshot.progress.state.progress_digest
        {
            return Err(TurnError::Conflict);
        }
        let bytes = avencrew_contracts::runtime::record_bytes(result)
            .map_err(|_| TurnError::InvalidSeed)?;
        if bytes.len() > 1_048_576 {
            return Err(TurnError::InvalidSeed);
        }
        // Response and progress have separately allocated immutable locators.
        self.put_canonical(access.workspace, response_id, &bytes)?;
        let mut progress = snapshot.progress;
        progress.state = state.clone();
        let progress_bytes = progress_bytes(access.turn, snapshot.version + 1, &progress)?;
        self.put_canonical(access.workspace, event, &progress_bytes)?;
        let identity = self
            .verified_identity(access.workspace, access.actor)
            .await?;
        let mut tx = self.connection.begin().await?;
        identity.check_live(&mut tx).await?;
        blob(
            &mut tx,
            access.workspace,
            access.actor,
            response_id,
            &bytes,
            MODEL_MEDIA,
            access.now.unix_micros(),
        )
        .await?;
        let (input, output) = usage(result);
        let changed = sqlx::query("UPDATE local_model_calls SET state=?,response_blob_id=?,input_tokens=?,output_tokens=?,ended_at_us=?,error_code=? WHERE workspace_id=? AND id=? AND state='intent'")
            .bind(if result.is_ok() {"response"} else {"unknown"}).bind(response_id.as_str()).bind(input).bind(output).bind(access.now.unix_micros()).bind(result.as_ref().err().and_then(|f|serde_json::to_value(f.code).ok().and_then(|v|v.as_str().map(str::to_owned)))).bind(access.workspace.as_str()).bind(step.model_call_id.as_str()).execute(&mut *tx).await?;
        if changed.rows_affected() != 1 {
            return Err(TurnError::Conflict);
        }
        budget::record_usage(
            &mut tx,
            &access,
            &seed,
            budget::Measurement {
                id: response_id,
                invocation: None,
                meter: "model_input_tokens",
                source: &step.model_call_id,
                unit: "token",
                quantity: input.map(|v| v as u64),
                price: &step.model_call_id,
            },
        )
        .await?;
        budget::record_usage(
            &mut tx,
            &access,
            &seed,
            budget::Measurement {
                id: event,
                invocation: None,
                meter: "model_output_tokens",
                source: &step.model_call_id,
                unit: "token",
                quantity: output.map(|v| v as u64),
                price: &step.model_call_id,
            },
        )
        .await?;
        save_progress(
            &mut tx,
            &access,
            &seed,
            snapshot.version,
            event,
            "execution.model_response",
            &progress_bytes,
        )
        .await?;
        #[cfg(test)]
        tests::barrier(&self.status.database_path, "response_before_commit")?;
        tx.commit().await?;
        #[cfg(test)]
        tests::barrier(&self.status.database_path, "response_after_commit")?;
        Ok(())
    }

    pub async fn authorize_local_execution(
        &mut self,
        a: &LocalExecutionAccess<'_>,
        retain_only: bool,
    ) -> Result<avencrew_contracts::wire::RpcRequestHarnessStartTurnBody, TurnError> {
        let retained = self.read_local_turn(a.workspace, a.actor, a.turn).await?;
        let identity = self.verified_identity(a.workspace, a.actor).await?;
        let run = run_row(
            &mut self.connection,
            a.workspace,
            &retained.seed.run_spec.run_id,
            a.actor,
        )
        .await?;
        require_local(&run, identity.device())?;
        let seed = retained.seed;
        if run.epoch != seed.lease.epoch.value() as i64
            || seed.lease.authority != ExecutionAuthority::Local
            || seed.run_spec.placement != Placement::Local
            || seed.lease.environment_id.0.is_some()
            || seed.run_spec.project_id.0.is_some()
            || seed.run_spec.worker_id.0.is_some()
            || !matches!(
                run.status.as_str(),
                "preparing" | "running" | "paused" | "cancelling" | "waiting" | "blocked"
            )
        {
            return Err(TurnError::StaleAuthority);
        }
        if !retain_only {
            if run.cancelled {
                return Err(TurnError::Cancelled);
            }
            if seed.lease.boot_id != *a.boot
                || seed.lease.expires_at <= *a.now
                || seed.lease.monotonic_deadline_ns.value() <= a.monotonic_now.value()
                || run.accepted != run.applied
            {
                return Err(TurnError::StaleAuthority);
            }
        }
        Ok(seed)
    }
}

fn check_lease_cut(
    a: &LocalExecutionAccess<'_>,
    seed: &avencrew_contracts::wire::RpcRequestHarnessStartTurnBody,
) -> Result<(), TurnError> {
    let elapsed = a.sampled_at.elapsed();
    if *a.boot != seed.lease.boot_id
        || i128::from(a.now.unix_micros()) + elapsed.as_micros() as i128
            >= i128::from(seed.lease.expires_at.unix_micros())
        || u128::from(a.monotonic_now.value()) + elapsed.as_nanos()
            >= u128::from(seed.lease.monotonic_deadline_ns.value())
    {
        return Err(TurnError::StaleAuthority);
    }
    Ok(())
}
async fn check_dispatch_cut(
    conn: &mut SqliteConnection,
    a: &LocalExecutionAccess<'_>,
    seed: &avencrew_contracts::wire::RpcRequestHarnessStartTurnBody,
) -> Result<(), TurnError> {
    check_lease_cut(a, seed)?;
    let row = run_row(conn, a.workspace, &seed.run_spec.run_id, a.actor).await?;
    if row.cancelled {
        return Err(TurnError::Cancelled);
    }
    if row.epoch != seed.lease.epoch.value() as i64
        || row.authority != "local"
        || row.accepted != row.applied
        || !matches!(row.status.as_str(), "preparing" | "running")
    {
        return Err(TurnError::StaleAuthority);
    }
    Ok(())
}
async fn save_progress(
    conn: &mut SqliteConnection,
    a: &LocalExecutionAccess<'_>,
    seed: &avencrew_contracts::wire::RpcRequestHarnessStartTurnBody,
    version: u64,
    event: &DomainId,
    kind: &str,
    bytes: &[u8],
) -> Result<(), TurnError> {
    blob(
        conn,
        a.workspace,
        a.actor,
        event,
        bytes,
        PROGRESS_MEDIA,
        a.now.unix_micros(),
    )
    .await?;
    let changed = sqlx::query("UPDATE local_turns SET runtime_version=runtime_version+1,runtime_blob_id=? WHERE workspace_id=? AND id=? AND runtime_version=?")
        .bind(event.as_str()).bind(a.workspace.as_str()).bind(a.turn.as_str()).bind(version as i64).execute(&mut *conn).await?;
    if changed.rows_affected() != 1 {
        return Err(TurnError::Conflict);
    }
    let row = run_row(conn, a.workspace, &seed.run_spec.run_id, a.actor).await?;
    sqlx::query("INSERT INTO local_events (id,workspace_id,created_at_us,run_id,seq,attempt_id,epoch,event_type,schema_version,occurred_at_us,payload_blob_id,command_id) VALUES (?,?,?,?,?,?,?,?,1,?,?,NULL)")
        .bind(event.as_str()).bind(a.workspace.as_str()).bind(a.now.unix_micros()).bind(seed.run_spec.run_id.as_str()).bind(next(row.event)?).bind(seed.lease.attempt_id.as_str()).bind(seed.lease.epoch.value() as i64).bind(kind).bind(a.now.unix_micros()).bind(event.as_str()).execute(&mut *conn).await?;
    let changed=sqlx::query("UPDATE local_runs SET last_event_seq=last_event_seq+1,row_version=row_version+1 WHERE workspace_id=? AND id=? AND fencing_epoch=?")
        .bind(a.workspace.as_str()).bind(seed.run_spec.run_id.as_str()).bind(seed.lease.epoch.value() as i64).execute(conn).await?;
    if changed.rows_affected() != 1 {
        return Err(TurnError::StaleAuthority);
    }
    Ok(())
}

async fn append_execution_event(
    conn: &mut SqliteConnection,
    a: &LocalExecutionAccess<'_>,
    seed: &avencrew_contracts::wire::RpcRequestHarnessStartTurnBody,
    event: &DomainId,
    kind: &str,
    payload: &DomainId,
) -> Result<(), TurnError> {
    let row = run_row(conn, a.workspace, &seed.run_spec.run_id, a.actor).await?;
    if row.epoch != seed.lease.epoch.value() as i64 {
        return Err(TurnError::StaleAuthority);
    }
    sqlx::query("INSERT INTO local_events (id,workspace_id,created_at_us,run_id,seq,attempt_id,epoch,event_type,schema_version,occurred_at_us,payload_blob_id,command_id) VALUES (?,?,?,?,?,?,?,?,1,?,?,NULL)")
        .bind(event.as_str()).bind(a.workspace.as_str()).bind(a.now.unix_micros()).bind(seed.run_spec.run_id.as_str()).bind(next(row.event)?).bind(seed.lease.attempt_id.as_str()).bind(seed.lease.epoch.value() as i64).bind(kind).bind(a.now.unix_micros()).bind(payload.as_str()).execute(&mut *conn).await?;
    let changed=sqlx::query("UPDATE local_runs SET last_event_seq=last_event_seq+1,row_version=row_version+1 WHERE workspace_id=? AND id=? AND fencing_epoch=?").bind(a.workspace.as_str()).bind(seed.run_spec.run_id.as_str()).bind(seed.lease.epoch.value() as i64).execute(conn).await?;
    if changed.rows_affected() != 1 {
        return Err(TurnError::StaleAuthority);
    }
    Ok(())
}
fn progress_bytes(
    turn: &DomainId,
    version: u64,
    progress: &LocalTurnProgress,
) -> Result<Vec<u8>, TurnError> {
    Ok(canonical(&ProgressRecord {
        schema_version: 1,
        turn_id: turn.clone(),
        version,
        progress: progress.clone(),
        input_dispositions: vec![],
        observed_invocation_ids: vec![],
    })?)
}
fn usage(result: &ModelCompletion) -> (Option<i64>, Option<i64>) {
    let usage = match result {
        Ok(r) => &r.usage,
        Err(r) => &r.usage,
    };
    match usage {
        FixtureUsage::Unknown => (None, None),
        FixtureUsage::Known {
            input_tokens,
            output_tokens,
        } => (
            Some(input_tokens.value() as i64),
            Some(output_tokens.value() as i64),
        ),
    }
}
fn validate_completion(step: &LocalStep, result: &ModelCompletion) -> Result<(), TurnError> {
    if let Ok(result) = result {
        result
            .response
            .validate()
            .map_err(|_| TurnError::InvalidSeed)?;
        if result.response.step_id != step.binding.step_id
            || result.response.model_call_id != step.model_call_id
            || result.proposal_identities.len() > 100
            || (result.response.finish != NormalizedModelResponseFinish::Tools
                && !result.proposal_identities.is_empty())
        {
            return Err(TurnError::InvalidSeed);
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests;
