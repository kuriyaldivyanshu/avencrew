//! Read-only replay of exact control dispositions. Never grants execution.
use super::*;

impl LocalStore {
    pub(super) async fn verify_local_control_history(
        &mut self,
        w: &DomainId,
        actor: &DomainId,
        turn: &DomainId,
    ) -> Result<LocalExecutionSnapshot, TurnError> {
        let retained = self.read_local_turn(w, actor, turn).await?;
        let inputs = self.local_execution_inputs(w, actor, turn).await?;
        let snap = self.local_execution_snapshot(w, actor, turn).await?;
        let run = run_row(
            &mut self.connection,
            w,
            &retained.seed.run_spec.run_id,
            actor,
        )
        .await?;
        let commands=sqlx::query("SELECT id,state,kind,(SELECT seq FROM local_events e WHERE e.workspace_id=c.workspace_id AND e.run_id=c.run_id AND e.command_id=c.id AND e.event_type='control.accepted') AS accepted_event FROM local_commands c WHERE workspace_id=? AND run_id=? ORDER BY seq LIMIT 1025")
            .bind(w.as_str()).bind(retained.seed.run_spec.run_id.as_str()).fetch_all(&mut self.connection).await?;
        let events=sqlx::query("SELECT id,seq,event_type,payload_blob_id,attempt_id,epoch FROM local_events WHERE workspace_id=? AND run_id=? ORDER BY seq LIMIT 2049")
            .bind(w.as_str()).bind(retained.seed.run_spec.run_id.as_str()).fetch_all(&mut self.connection).await?;
        if events.len() > 2048 || events.len() as i64 != run.event || commands.len() != inputs.len()
        {
            return Err(TurnError::Conflict);
        }
        if run.cancelled != inputs.iter().any(|c| matches!(c, Command::Stop { .. })) {
            return Err(TurnError::Conflict);
        }
        let (mut applied, mut version, mut saw_outcome, mut replay_bytes) =
            (0usize, 0u64, false, 0usize);
        let mut latest = None;
        for (n, event) in events.iter().enumerate() {
            if event.try_get::<i64, _>("seq")? != n as i64 + 1 {
                return Err(TurnError::Conflict);
            }
            let kind: String = event.try_get("event_type")?;
            if !matches!(
                kind.as_str(),
                "execution.model_intent"
                    | "execution.model_response"
                    | "execution.proposals"
                    | "execution.controls_applied"
                    | "execution.observed"
                    | "execution.outcome"
            ) {
                continue;
            }
            if saw_outcome
                || event.try_get::<Option<String>, _>("attempt_id")?.as_deref()
                    != Some(retained.seed.lease.attempt_id.as_str())
                || event.try_get::<i64, _>("epoch")? != retained.seed.lease.epoch.value() as i64
            {
                return Err(TurnError::Conflict);
            }
            let id = DomainId::new(event.try_get::<String, _>("payload_blob_id")?)
                .map_err(|_| TurnError::Conflict)?;
            if id.as_str() != event.try_get::<String, _>("id")? {
                return Err(TurnError::Conflict);
            }
            let bytes = self
                .read_registered_blob(w, actor, &id, PROGRESS_MEDIA)
                .await?;
            let record: ProgressRecord =
                serde_json::from_slice(&bytes).map_err(|_| TurnError::Conflict)?;
            replay_bytes += bytes.len();
            if replay_bytes > 8 * 1024 * 1024 {
                return Err(TurnError::Conflict);
            }
            version += 1;
            if bytes != canonical(&record)?
                || record.schema_version != 1
                || record.turn_id != *turn
                || record.version != version
            {
                return Err(TurnError::Conflict);
            }
            if kind == "execution.controls_applied" {
                let end = record.progress.state.applied_input_cut.value() as usize;
                if end <= applied
                    || end > commands.len()
                    || record.input_dispositions.len() != end - applied
                {
                    return Err(TurnError::Conflict);
                }
                // Stop priority is determined by the accepted prefix at this
                // application event, never by a stop accepted later in history.
                let stopped = commands[..end]
                    .iter()
                    .any(|c| c.try_get::<String, _>("kind").is_ok_and(|k| k == "cancel"));
                for (offset, disposition) in record.input_dispositions.iter().enumerate() {
                    let index = applied + offset;
                    let c = &commands[index];
                    let superseded = stopped && c.try_get::<String, _>("kind")? != "cancel";
                    if c.try_get::<i64, _>("accepted_event")? >= event.try_get::<i64, _>("seq")?
                        || disposition.command_id.as_str() != c.try_get::<String, _>("id")?
                        || disposition.sequence.value() != index as u64 + 1
                        || disposition.disposition
                            != if superseded {
                                Disposition::Superseded
                            } else {
                                Disposition::Applied
                            }
                        || c.try_get::<String, _>("state")?
                            != if superseded { "rejected" } else { "applied" }
                    {
                        return Err(TurnError::Conflict);
                    }
                }
                applied = end;
            } else if !record.input_dispositions.is_empty() {
                return Err(TurnError::Conflict);
            }
            if record.progress.state.applied_input_cut.value() != applied as u64 {
                return Err(TurnError::Conflict);
            }
            saw_outcome = record.progress.outcome.is_some();
            if saw_outcome != (kind == "execution.outcome") {
                return Err(TurnError::Conflict);
            }
            latest = Some(record.progress);
        }
        if applied as i64 != run.applied
            || version != snap.version
            || latest.as_ref().is_some_and(|p| p != &snap.progress)
            || commands[applied..].iter().any(|c| {
                !c.try_get::<String, _>("state")
                    .is_ok_and(|s| s == "accepted")
            })
        {
            return Err(TurnError::Conflict);
        }
        Ok(snap)
    }

    /// Validate the finite fixture ledger before returning even a historical
    /// outcome. Reconciliation reads never apply controls, renew or release work.
    pub async fn reconcile_local_execution(
        &mut self,
        w: &DomainId,
        actor: &DomainId,
        turn: &DomainId,
    ) -> Result<LocalExecutionSnapshot, TurnError> {
        let snap = self.verify_local_control_history(w, actor, turn).await?;
        self.local_execution_context(w, actor, turn).await?;
        let seed = self.read_local_turn(w, actor, turn).await?.seed;
        let unsupported:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM local_events WHERE workspace_id=? AND run_id=? AND event_type NOT IN ('run.accepted','control.accepted','turn.prepared','checkpoint.committed','execution.model_intent','execution.model_response','execution.proposals','execution.controls_applied','execution.observed','execution.outcome','execution.dispatch','execution.tool_result','execution.tool_unknown'))")
            .bind(w.as_str()).bind(seed.run_spec.run_id.as_str()).fetch_one(&mut self.connection).await?;
        if unsupported {
            return Err(TurnError::Unsupported);
        }
        if let Some(checkpoint) = &snap.current_checkpoint {
            let saved = self
                .read_checkpoint(w, actor, &seed.run_spec.run_id, checkpoint)
                .await?;
            if saved.coverage.context != "exact_fixture_ledger" {
                return Err(TurnError::Unsupported);
            }
        }
        let identity = self.verified_identity(w, actor).await?;
        let run = run_row(&mut self.connection, w, &seed.run_spec.run_id, actor).await?;
        require_local(&run, identity.device())?;
        if run.epoch != seed.lease.epoch.value() as i64 {
            return Err(TurnError::StaleAuthority);
        }
        // No clock check here: historical cancelled/blocked metadata remains
        // inspectable after expiry, but cannot turn into an execution grant.
        Ok(snap)
    }
}
