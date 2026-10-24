use super::*;

pub struct LocalControlSnapshot {
    pub applied_input_cut: Counter,
    pub stopped: bool,
    pub paused: bool,
    pub inputs: Vec<Command>,
}

impl LocalStore {
    /// Digest verified semantic evidence, excluding fresh invocation IDs so a
    /// repeated read of the same source/version/result cannot fake progress.
    pub(super) async fn execution_progress_digest(
        &mut self,
        w: &DomainId,
        actor: &DomainId,
        seed: &avencrew_contracts::wire::RpcRequestHarnessStartTurnBody,
    ) -> Result<avencrew_contracts::scalars::Digest, TurnError> {
        use avencrew_contracts::{
            runtime::{content_digest, record_bytes},
            wire::ToolResult,
        };
        let rows:Vec<String>=sqlx::query_scalar("SELECT id FROM local_invocations WHERE workspace_id=? AND run_id=? AND state='result' ORDER BY id LIMIT 101")
            .bind(w.as_str()).bind(seed.run_spec.run_id.as_str()).fetch_all(&mut self.connection).await?;
        if rows.len() > 100 {
            return Err(TurnError::Conflict);
        }
        let mut evidence = std::collections::BTreeSet::new();
        for id in rows {
            let id = DomainId::new(id).map_err(|_| TurnError::Conflict)?;
            let proposal = self.read_local_proposal(w, actor, &id).await?;
            let result = self.read_local_tool_result(w, actor, &id).await?;
            if let ToolResult::Succeeded {
                data,
                artifact_refs,
                receipt_ref,
                ..
            } = result
            {
                if !data.value.is_null()
                    || !artifact_refs.as_slice().is_empty()
                    || receipt_ref.0.is_some()
                {
                    let bytes=record_bytes(&serde_json::json!({"adapter":proposal.adapter_digest,"binding":proposal.context.binding_id,"arguments":proposal.arguments_digest,"data":data,"artifacts":artifact_refs,"receipt":receipt_ref})).map_err(|_|TurnError::Conflict)?;
                    evidence.insert(content_digest(&bytes).as_str().to_owned());
                }
            }
        }
        let artifacts=sqlx::query("SELECT a.id,a.current_version_id FROM local_artifacts a WHERE a.workspace_id=? AND a.owner_actor_id=? AND a.id IN (SELECT artifact_id FROM local_artifact_versions WHERE workspace_id=? AND run_id=?) ORDER BY a.id LIMIT 129")
            .bind(w.as_str()).bind(actor.as_str()).bind(w.as_str()).bind(seed.run_spec.run_id.as_str()).fetch_all(&mut self.connection).await?;
        if artifacts.len() > 128 {
            return Err(TurnError::Conflict);
        }
        let mut versions = Vec::new();
        for row in artifacts {
            let id =
                DomainId::new(row.try_get::<String, _>("id")?).map_err(|_| TurnError::Conflict)?;
            let version = DomainId::new(row.try_get::<String, _>("current_version_id")?)
                .map_err(|_| TurnError::Conflict)?;
            let saved = self.read_local_artifact(w, actor, &id, &version).await?;
            versions.push(saved.receipt);
        }
        Ok(content_digest(&record_bytes(&serde_json::json!({"domain":"avencrew.fixture-progress/1","objective":seed.run_spec.objective_version_id,"audience":seed.run_spec.audience_policy_id,"checks":seed.run_spec.check_refs,"evidence":evidence,"versions":versions})).map_err(|_|TurnError::Conflict)?))
    }
    /// Cheap target-local cancellation observation; no disposition or lease change.
    pub async fn local_execution_stop_requested(
        &mut self,
        w: &DomainId,
        actor: &DomainId,
        turn: &DomainId,
    ) -> Result<bool, TurnError> {
        let seed = self.read_local_turn(w, actor, turn).await?.seed;
        let row = run_row(&mut self.connection, w, &seed.run_spec.run_id, actor).await?;
        Ok(row.cancelled)
    }
    /// Reads exact accepted input, checking its immutable original acceptance.
    pub async fn local_execution_inputs(
        &mut self,
        w: &DomainId,
        a: &DomainId,
        turn: &DomainId,
    ) -> Result<Vec<Command>, TurnError> {
        let seed = self.read_local_turn(w, a, turn).await?.seed;
        let run = run_row(&mut self.connection, w, &seed.run_spec.run_id, a).await?;
        let rows=sqlx::query("SELECT seq,payload_blob_id FROM local_commands WHERE workspace_id=? AND run_id=? ORDER BY seq LIMIT 1025")
            .bind(w.as_str()).bind(seed.run_spec.run_id.as_str()).fetch_all(&mut self.connection).await?;
        if rows.len() > 1024 || rows.len() as i64 != run.accepted {
            return Err(TurnError::Conflict);
        }
        let mut inputs = Vec::new();
        let mut total = 0;
        for (n, row) in rows.iter().enumerate() {
            if row.try_get::<i64, _>("seq")? != n as i64 + 1 {
                return Err(TurnError::Conflict);
            }
            let id = DomainId::new(row.try_get::<String, _>("payload_blob_id")?)
                .map_err(|_| TurnError::Conflict)?;
            let bytes = self.read_registered_blob(w, a, &id, COMMAND_MEDIA).await?;
            total += bytes.len();
            if total > 8 * 1024 * 1024 {
                return Err(TurnError::Conflict);
            }
            let (command, canonical) = decode(&bytes)?;
            if canonical != bytes
                || self
                    .existing_command(w, a, &command, &bytes)
                    .await?
                    .is_none()
            {
                return Err(TurnError::Conflict);
            }
            inputs.push(command);
        }
        Ok(inputs)
    }

    /// Safe fixture boundary only: records input dispositions, never asserts process cleanup.
    pub async fn apply_local_controls(
        &mut self,
        a: LocalExecutionAccess<'_>,
        event: &DomainId,
    ) -> Result<LocalControlSnapshot, TurnError> {
        let seed = self.authorize_local_execution(&a, true).await?;
        let snap = self
            .verify_local_control_history(a.workspace, a.actor, a.turn)
            .await?;
        let row = run_row(
            &mut self.connection,
            a.workspace,
            &seed.run_spec.run_id,
            a.actor,
        )
        .await?;
        if row.accepted == row.applied {
            let paused: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM local_commands WHERE workspace_id=? AND run_id=? AND kind='pause' AND state='applied')")
                .bind(a.workspace.as_str()).bind(seed.run_spec.run_id.as_str()).fetch_one(&mut self.connection).await?;
            return Ok(LocalControlSnapshot {
                applied_input_cut: Counter::new(row.applied.to_string())
                    .map_err(|_| TurnError::Conflict)?,
                stopped: row.cancelled,
                paused,
                inputs: vec![],
            });
        }
        let inputs = self
            .local_execution_inputs(a.workspace, a.actor, a.turn)
            .await?;
        let paused = inputs.iter().any(|c| matches!(c, Command::Pause { .. }));
        let stopped = row.cancelled;
        if snap.progress.outcome.is_some() {
            return Err(TurnError::Conflict);
        }
        let mut progress = snap.progress;
        progress.state.applied_input_cut =
            Counter::new(row.accepted.to_string()).map_err(|_| TurnError::Conflict)?;
        // Persist typed exact dispositions, with stop priority over every
        // unapplied business command in this accepted prefix.
        let dispositions = inputs
            .iter()
            .enumerate()
            .skip(row.applied as usize)
            .map(|(n, c)| {
                Ok(InputDisposition {
                    command_id: control(c)?.0.clone(),
                    sequence: positive(n as i64 + 1)?,
                    disposition: if stopped && !matches!(c, Command::Stop { .. }) {
                        Disposition::Superseded
                    } else {
                        Disposition::Applied
                    },
                })
            })
            .collect::<Result<Vec<_>, ControlError>>()?;
        let bytes = canonical(&ProgressRecord {
            schema_version: 1,
            turn_id: a.turn.clone(),
            version: snap.version + 1,
            progress,
            input_dispositions: dispositions,
            observed_invocation_ids: vec![],
        })?;
        self.put_canonical(a.workspace, event, &bytes)?;
        let identity = self.verified_identity(a.workspace, a.actor).await?;
        let mut tx = self.connection.begin().await?;
        identity.check_live(&mut tx).await?;
        let live = run_row(&mut tx, a.workspace, &seed.run_spec.run_id, a.actor).await?;
        if live.accepted != row.accepted
            || live.applied != row.applied
            || live.cancelled != row.cancelled
            || live.epoch != row.epoch
        {
            return Err(TurnError::Conflict);
        }
        let changed=sqlx::query("UPDATE local_commands SET state=CASE WHEN ?=1 AND kind<>'cancel' THEN 'rejected' ELSE 'applied' END WHERE workspace_id=? AND run_id=? AND seq>? AND seq<=? AND state='accepted'")
            .bind(stopped).bind(a.workspace.as_str()).bind(seed.run_spec.run_id.as_str()).bind(row.applied).bind(row.accepted).execute(&mut *tx).await?;
        if changed.rows_affected() != (row.accepted - row.applied) as u64 {
            return Err(TurnError::Conflict);
        }
        let changed=sqlx::query("UPDATE local_runs SET applied_command_seq=? WHERE workspace_id=? AND id=? AND fencing_epoch=? AND applied_command_seq=?")
            .bind(row.accepted)
            .bind(a.workspace.as_str())
            .bind(seed.run_spec.run_id.as_str())
            .bind(seed.lease.epoch.value() as i64).bind(row.applied)
            .execute(&mut *tx)
            .await?;
        if changed.rows_affected() != 1 {
            return Err(TurnError::StaleAuthority);
        }
        save_progress(
            &mut tx,
            &a,
            &seed,
            snap.version,
            event,
            "execution.controls_applied",
            &bytes,
        )
        .await?;
        #[cfg(test)]
        tests::barrier(&self.status.database_path, "controls_before_commit")?;
        tx.commit().await?;
        #[cfg(test)]
        tests::barrier(&self.status.database_path, "controls_after_commit")?;
        Ok(LocalControlSnapshot {
            applied_input_cut: Counter::new(row.accepted.to_string())
                .map_err(|_| TurnError::Conflict)?,
            paused,
            stopped,
            inputs,
        })
    }

    /// Exact observations are already retained in invocation rows. This commits the derived counters once.
    pub async fn observe_local_step(
        &mut self,
        a: LocalExecutionAccess<'_>,
        step: &DomainId,
        observations: &[DomainId],
        state: &TurnState,
        event: &DomainId,
    ) -> Result<avencrew_contracts::scalars::Digest, TurnError> {
        let seed = self.authorize_local_execution(&a, true).await?;
        let snap = self
            .local_execution_snapshot(a.workspace, a.actor, a.turn)
            .await?;
        if snap.progress.pending_step_id.as_ref() != Some(step)
            || state.steps_started != snap.progress.state.steps_started
            || state.applied_input_cut != snap.progress.state.applied_input_cut
            || state.no_progress_steps != snap.progress.state.no_progress_steps
            || state.progress_digest != snap.progress.state.progress_digest
            || state.active_elapsed_ms < snap.progress.state.active_elapsed_ms
        {
            return Err(TurnError::Conflict);
        }
        let completion = snap
            .model
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .ok_or(TurnError::Conflict)?;
        if observations.len() > completion.proposal_identities.len()
            || observations
                .iter()
                .zip(&completion.proposal_identities)
                .any(|(id, p)| *id != p.invocation_id)
        {
            return Err(TurnError::Conflict);
        }
        for id in observations {
            let p = self.read_local_proposal(a.workspace, a.actor, id).await?;
            if p.context.step_id != *step {
                return Err(TurnError::Conflict);
            }
            let dispatched: bool = sqlx::query_scalar(
                "SELECT state='dispatched' FROM local_invocations WHERE workspace_id=? AND id=?",
            )
            .bind(a.workspace.as_str())
            .bind(id.as_str())
            .fetch_one(&mut self.connection)
            .await?;
            if dispatched {
                return Err(TurnError::Conflict);
            }
        }
        let digest = self
            .execution_progress_digest(a.workspace, a.actor, &seed)
            .await?;
        let mut progress = snap.progress;
        progress.state = state.clone();
        progress.state.no_progress_steps = if digest == state.progress_digest {
            state
                .no_progress_steps
                .checked_add(1)
                .filter(|v| *v <= 50)
                .ok_or(TurnError::Conflict)?
        } else {
            0
        };
        progress.state.progress_digest = digest.clone();
        progress.pending_step_id = None;
        let bytes = canonical(&ProgressRecord {
            schema_version: 1,
            turn_id: a.turn.clone(),
            version: snap.version + 1,
            progress,
            input_dispositions: vec![],
            observed_invocation_ids: observations.to_vec(),
        })?;
        self.put_canonical(a.workspace, event, &bytes)?;
        let identity = self.verified_identity(a.workspace, a.actor).await?;
        let mut tx = self.connection.begin().await?;
        identity.check_live(&mut tx).await?;
        let changed=sqlx::query("UPDATE local_steps SET state='observed' WHERE workspace_id=? AND id=? AND state='sealed'").bind(a.workspace.as_str()).bind(step.as_str()).execute(&mut *tx).await?;
        if changed.rows_affected() != 1 {
            return Err(TurnError::Conflict);
        }
        save_progress(
            &mut tx,
            &a,
            &seed,
            snap.version,
            event,
            "execution.observed",
            &bytes,
        )
        .await?;
        tx.commit().await?;
        Ok(digest)
    }
}
