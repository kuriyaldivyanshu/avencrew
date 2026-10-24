use super::*;

pub struct LocalControlSnapshot {
    pub applied_input_cut: Counter,
    pub stopped: bool,
    pub paused: bool,
    pub inputs: Vec<Command>,
}

impl LocalStore {
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
        let snap = self
            .local_execution_snapshot(a.workspace, a.actor, a.turn)
            .await?;
        let paused = inputs.iter().any(|c| matches!(c, Command::Pause { .. }));
        let stopped = row.cancelled;
        if row.accepted == row.applied {
            return Ok(LocalControlSnapshot {
                applied_input_cut: Counter::new(row.applied.to_string())
                    .map_err(|_| TurnError::Conflict)?,
                paused,
                stopped,
                inputs,
            });
        }
        if snap.progress.outcome.is_some() {
            return Err(TurnError::Conflict);
        }
        let mut progress = snap.progress;
        progress.state.applied_input_cut =
            Counter::new(row.accepted.to_string()).map_err(|_| TurnError::Conflict)?;
        let mut record = serde_json::to_value(ProgressRecord {
            schema_version: 1,
            turn_id: a.turn.clone(),
            version: snap.version + 1,
            progress,
            input_dispositions: vec![],
        })
        .map_err(|_| TurnError::Conflict)?;
        // The event retains exact per-command applied/superseded dispositions.
        let dispositions:Vec<_>=inputs.iter().enumerate().filter(|(n,_)|*n as i64>=row.applied).map(|(n,c)|{
            let superseded=stopped&&!matches!(c,Command::Stop{..});
            Ok(serde_json::json!({"command_id":control(c)?.0,"sequence":(n+1).to_string(),"disposition":if superseded{"superseded"}else{"applied"}}))
        }).collect::<Result<_,ControlError>>()?;
        record["input_dispositions"] = serde_json::json!(dispositions);
        let bytes = canonical(&record)?;
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
        sqlx::query("UPDATE local_commands SET state=CASE WHEN ?=1 AND kind<>'cancel' THEN 'rejected' ELSE 'applied' END WHERE workspace_id=? AND run_id=? AND seq>? AND seq<=? AND state='accepted'")
            .bind(stopped).bind(a.workspace.as_str()).bind(seed.run_spec.run_id.as_str()).bind(row.applied).bind(row.accepted).execute(&mut *tx).await?;
        sqlx::query("UPDATE local_runs SET applied_command_seq=? WHERE workspace_id=? AND id=?")
            .bind(row.accepted)
            .bind(a.workspace.as_str())
            .bind(seed.run_spec.run_id.as_str())
            .execute(&mut *tx)
            .await?;
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
        tx.commit().await?;
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
        {
            return Err(TurnError::Conflict);
        }
        // Reads alone do not establish business progress; no model text increments it.
        let digest = state.progress_digest.clone();
        let mut progress = snap.progress;
        progress.state = state.clone();
        progress.state.no_progress_steps = state
            .no_progress_steps
            .checked_add(1)
            .filter(|v| *v <= 50)
            .ok_or(TurnError::Conflict)?;
        progress.pending_step_id = None;
        let bytes = progress_bytes(a.turn, snap.version + 1, &progress)?;
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
