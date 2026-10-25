//! Read-only replay of exact control dispositions. Never grants execution.
use super::*;

pub(super) struct VerifiedHistory {
    pub snapshot: LocalExecutionSnapshot,
    pub inputs: super::controls::VerifiedInputs,
}

/// A single-operation restore result, never a reusable execution grant.
pub struct LocalExecutionRestore {
    pub snapshot: LocalExecutionSnapshot,
    pub context: serde_json::Value,
    pub paused: bool,
}

impl LocalStore {
    pub(super) async fn verify_local_control_history(
        &mut self,
        w: &DomainId,
        actor: &DomainId,
        turn: &DomainId,
    ) -> Result<VerifiedHistory, TurnError> {
        let _proof_scope = self.proof_read_scope();
        let retained = self.read_local_turn(w, actor, turn).await?;
        // The verifier deliberately supports exactly the admitted ordinal-one
        // fixture profile. Future multi-turn histories need a different replay.
        let turns: (i64, i64) = sqlx::query_as(
            "SELECT count(*),coalesce(max(ordinal),0) FROM local_turns WHERE workspace_id=? AND run_id=?"
        ).bind(w.as_str()).bind(retained.seed.run_spec.run_id.as_str())
            .fetch_one(&mut self.connection).await?;
        if turns != (1, 1) {
            return Err(TurnError::Unsupported);
        }
        let inputs = self.read_local_execution_inputs(w, actor, turn).await?;
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
        if events.len() > 2048
            || events.len() as i64 != run.event
            || commands.len() != inputs.commands.len()
        {
            return Err(TurnError::Conflict);
        }
        for c in &commands {
            if c.try_get::<Option<i64>, _>("accepted_event")?.is_none() {
                return Err(TurnError::Conflict);
            }
        }
        if run.cancelled
            != inputs
                .commands
                .iter()
                .any(|c| matches!(c, Command::Stop { .. }))
        {
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
            applied =
                verify_dispositions(&commands, &record, &kind, event.try_get("seq")?, applied)?;
            self.verify_observed_invocations(w, actor, turn, &kind, latest.as_ref(), &record)
                .await?;
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
        Ok(VerifiedHistory {
            snapshot: snap,
            inputs,
        })
    }

    /// Validate the finite fixture ledger before returning even a historical
    /// outcome. Reconciliation reads never apply controls, renew or release work.
    pub async fn reconcile_local_execution(
        &mut self,
        w: &DomainId,
        actor: &DomainId,
        turn: &DomainId,
    ) -> Result<LocalExecutionSnapshot, TurnError> {
        let _proof_scope = self.proof_read_scope();
        Ok(self
            .reconcile_local_execution_details(w, actor, turn)
            .await?
            .0
            .snapshot)
    }

    async fn reconcile_local_execution_details(
        &mut self,
        w: &DomainId,
        actor: &DomainId,
        turn: &DomainId,
    ) -> Result<(VerifiedHistory, serde_json::Value), TurnError> {
        let _proof_scope = self.proof_read_scope();
        let verified = self.verify_local_control_history(w, actor, turn).await?;
        let snap = &verified.snapshot;
        let context = self
            .verified_local_execution_context(w, actor, turn, snap, &verified.inputs)
            .await?;
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
        Ok((verified, context))
    }

    /// Verify once, apply pending controls with live transactional fences, and
    /// return the verified context for this restore. No cache survives the call.
    pub async fn restore_local_execution(
        &mut self,
        a: LocalExecutionAccess<'_>,
        event: &DomainId,
    ) -> Result<LocalExecutionRestore, TurnError> {
        let _proof_scope = self.proof_read_scope();
        let (verified, mut context) = self
            .reconcile_local_execution_details(a.workspace, a.actor, a.turn)
            .await?;
        let paused = verified
            .inputs
            .commands
            .iter()
            .any(|c| matches!(c, Command::Pause { .. }));
        if let Some(outcome) = &verified.snapshot.progress.outcome {
            if (verified.snapshot.cancellation_requested
                && !matches!(
                    outcome,
                    avencrew_contracts::wire::TurnOutcome::Cancelled { .. }
                ))
                || verified.inputs.commands.len() as u64
                    != verified.snapshot.progress.state.applied_input_cut.value()
            {
                return Err(TurnError::StaleAuthority);
            }
            return Ok(LocalExecutionRestore {
                snapshot: verified.snapshot,
                context,
                paused,
            });
        }
        let (w, actor, turn) = (a.workspace, a.actor, a.turn);
        let changed = verified.inputs.commands.len() as u64
            != verified.snapshot.progress.state.applied_input_cut.value();
        // Only refresh the projection when applying controls actually wrote it.
        if changed {
            self.apply_verified_local_controls(a, event, verified)
                .await?;
            let snapshot = self.local_execution_snapshot(w, actor, turn).await?;
            context["state"] =
                serde_json::to_value(&snapshot.progress.state).map_err(|_| TurnError::Conflict)?;
            Ok(LocalExecutionRestore {
                snapshot,
                context,
                paused,
            })
        } else {
            self.authorize_local_execution(&a, true).await?;
            Ok(LocalExecutionRestore {
                snapshot: verified.snapshot,
                context,
                paused,
            })
        }
    }

    async fn verify_observed_invocations(
        &mut self,
        w: &DomainId,
        actor: &DomainId,
        turn: &DomainId,
        kind: &str,
        prior: Option<&LocalTurnProgress>,
        record: &ProgressRecord,
    ) -> Result<(), TurnError> {
        if kind != "execution.observed" {
            return if record.observed_invocation_ids.is_empty() {
                Ok(())
            } else {
                Err(TurnError::Conflict)
            };
        }
        if record.progress.pending_step_id.is_some() {
            return Err(TurnError::Conflict);
        }
        let step_id = prior
            .and_then(|p| p.pending_step_id.as_ref())
            .ok_or(TurnError::Conflict)?;
        let step = self.read_local_step(w, actor, turn, step_id).await?;
        let completion = self
            .read_local_model_completion(w, actor, &step)
            .await?
            .and_then(Result::ok)
            .ok_or(TurnError::Conflict)?;
        if record.observed_invocation_ids.len() > completion.proposal_identities.len() {
            return Err(TurnError::Conflict);
        }
        for (id, proposal) in record
            .observed_invocation_ids
            .iter()
            .zip(&completion.proposal_identities)
        {
            if *id != proposal.invocation_id {
                return Err(TurnError::Conflict);
            }
            let valid: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM local_invocations WHERE workspace_id=? AND id=? AND step_id=? AND state IN ('proposed','result','unknown'))"
            ).bind(w.as_str()).bind(id.as_str()).bind(step_id.as_str()).fetch_one(&mut self.connection).await?;
            if !valid {
                return Err(TurnError::Conflict);
            }
        }
        Ok(())
    }
}

fn verify_dispositions(
    commands: &[sqlx::sqlite::SqliteRow],
    record: &ProgressRecord,
    kind: &str,
    event_seq: i64,
    applied: usize,
) -> Result<usize, TurnError> {
    let end = record.progress.state.applied_input_cut.value() as usize;
    if kind != "execution.controls_applied" {
        return if record.input_dispositions.is_empty() && end == applied {
            Ok(applied)
        } else {
            Err(TurnError::Conflict)
        };
    }
    if end <= applied || end > commands.len() || record.input_dispositions.len() != end - applied {
        return Err(TurnError::Conflict);
    }
    // Stop priority belongs to the accepted prefix at this application event;
    // a stop accepted later never supersedes already applied steering.
    let stopped = commands[..end]
        .iter()
        .any(|c| c.try_get::<String, _>("kind").is_ok_and(|k| k == "cancel"));
    for (offset, disposition) in record.input_dispositions.iter().enumerate() {
        let index = applied + offset;
        let c = &commands[index];
        let superseded = stopped && c.try_get::<String, _>("kind")? != "cancel";
        if c.try_get::<Option<i64>, _>("accepted_event")?
            .ok_or(TurnError::Conflict)?
            >= event_seq
            || disposition.command_id.as_str() != c.try_get::<String, _>("id")?
            || disposition.sequence.value() != index as u64 + 1
            || disposition.disposition
                != if superseded {
                    Disposition::Superseded
                } else {
                    Disposition::Applied
                }
            || c.try_get::<String, _>("state")? != if superseded { "rejected" } else { "applied" }
        {
            return Err(TurnError::Conflict);
        }
    }
    Ok(end)
}
