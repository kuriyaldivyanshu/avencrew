//! Conservative fixture resource accounting. Unknown measurements are never zeroed.
use super::*;
const TOKEN_UNITS: u128 = 8 * 1024 * 1024;
const BLOB_BYTES: u128 = 64 * 1024 * 1024;
pub(super) const TOOL_CALLS: i64 = 100;

#[derive(Debug, Serialize)]
pub struct LocalExecutionBudget {
    pub schema_version: &'static str,
    pub monetary_cap_micro: Counter,
    pub fixed_rate_micro: Counter,
    pub model_calls: Counter,
    pub known_input_tokens: Option<Counter>,
    pub known_output_tokens: Option<Counter>,
    pub unknown_model_calls: Counter,
    pub tool_proposals: Counter,
    pub dispatches: Counter,
    pub retained_blob_bytes: Counter,
    pub blob_inventory_complete: bool,
    pub reserved_token_units: Option<Counter>,
    pub token_unit_limit: Counter,
    pub tool_limit: Counter,
    pub blob_byte_limit: Counter,
    pub overdrawn: bool,
    pub can_start_model: bool,
    pub can_dispatch: bool,
}
fn bounded(n: u128) -> Option<Counter> {
    Counter::new(n.to_string()).ok()
}
fn count(n: u128) -> Result<Counter, TurnError> {
    bounded(n).ok_or(TurnError::Conflict)
}
impl LocalStore {
    // Count the private namespace, including registered and unregistered crash
    // orphans. A failed publication cannot hide disk use from the next admission.
    fn fixture_blob_inventory(&self, w: &DomainId) -> Result<(u128, bool), TurnError> {
        use std::os::unix::fs::MetadataExt;
        let root = self
            .status
            .database_path
            .parent()
            .ok_or(TurnError::Conflict)?;
        let owner = std::fs::symlink_metadata(root)
            .map_err(StoreError::from)?
            .uid();
        let namespace = root.join("blobs").join(w.as_str());
        let mut bytes = 0u128;
        let mut count = 0usize;
        let mut pending = vec![(namespace, 0usize)];
        while let Some((directory, depth)) = pending.pop() {
            let metadata = match std::fs::symlink_metadata(&directory) {
                Ok(m) => m,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(StoreError::from(e).into()),
            };
            if !metadata.is_dir() || metadata.uid() != owner || metadata.mode() & 0o777 != 0o700 {
                return Err(TurnError::Conflict);
            }
            for entry in std::fs::read_dir(&directory).map_err(StoreError::from)? {
                let entry = entry.map_err(StoreError::from)?;
                let m = std::fs::symlink_metadata(entry.path()).map_err(StoreError::from)?;
                count += 1;
                if count > 4096 {
                    return Ok((bytes, false));
                }
                if m.uid() != owner {
                    return Err(TurnError::Conflict);
                }
                if m.is_dir() {
                    if depth >= 3 {
                        return Ok((bytes, false));
                    }
                    pending.push((entry.path(), depth + 1));
                    continue;
                }
                if !m.is_file() || m.mode() & 0o777 != 0o600 {
                    return Err(TurnError::Conflict);
                }
                bytes += u128::from(m.len());
            }
        }
        Ok((bytes, true))
    }

    /// Scope is the run, so reopening or a later turn cannot mint a new allowance.
    pub async fn local_execution_budget(
        &mut self,
        w: &DomainId,
        actor: &DomainId,
        turn: &DomainId,
    ) -> Result<LocalExecutionBudget, TurnError> {
        let _proof_scope = self.proof_read_scope();
        let seed = self.read_local_turn(w, actor, turn).await?.seed;
        let calls=sqlx::query("SELECT c.input_tokens,c.output_tokens,b.byte_size FROM local_model_calls c JOIN local_steps s ON s.workspace_id=c.workspace_id AND s.id=c.step_id JOIN local_blobs b ON b.workspace_id=s.workspace_id AND b.id=s.context_blob_id WHERE s.workspace_id=? AND s.run_id=? ORDER BY s.step_ordinal LIMIT 51")
            .bind(w.as_str()).bind(seed.run_spec.run_id.as_str()).fetch_all(&mut self.connection).await?;
        if calls.len() > 50 {
            return Err(TurnError::Conflict);
        }
        let (mut input, mut output, mut unknown, mut reserved) = (0u128, 0u128, 0u128, 0u128);
        for row in &calls {
            let i: Option<i64> = row.try_get("input_tokens")?;
            let o: Option<i64> = row.try_get("output_tokens")?;
            if i.is_none() || o.is_none() {
                unknown += 1;
            }
            input += i.unwrap_or(0) as u128;
            output += o.unwrap_or(0) as u128;
            let reservation = row.try_get::<i64, _>("byte_size")? as u128
                + u128::from(seed.limits.output_bytes.value());
            let measured = i.unwrap_or(0) as u128 + o.unwrap_or(0) as u128;
            reserved += reservation.max(measured);
        }
        let tools: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM local_invocations WHERE workspace_id=? AND run_id=?",
        )
        .bind(w.as_str())
        .bind(seed.run_spec.run_id.as_str())
        .fetch_one(&mut self.connection)
        .await?;
        let dispatches:i64=sqlx::query_scalar("SELECT count(*) FROM local_dispatches d JOIN local_invocations i ON i.workspace_id=d.workspace_id AND i.id=d.invocation_id WHERE i.workspace_id=? AND i.run_id=?").bind(w.as_str()).bind(seed.run_spec.run_id.as_str()).fetch_one(&mut self.connection).await?;
        let (bytes, complete) = self.fixture_blob_inventory(w)?;
        let overdrawn =
            reserved > TOKEN_UNITS || bytes > BLOB_BYTES || tools > TOOL_CALLS || !complete;
        let reserve = u128::from(seed.limits.output_bytes.value()) + 1_048_576;
        Ok(LocalExecutionBudget {
            schema_version: "avencrew.fixture-budget/1",
            monetary_cap_micro: count(0)?,
            fixed_rate_micro: count(0)?,
            model_calls: count(calls.len() as u128)?,
            known_input_tokens: bounded(input),
            known_output_tokens: bounded(output),
            unknown_model_calls: count(unknown)?,
            tool_proposals: count(tools as u128)?,
            dispatches: count(dispatches as u128)?,
            retained_blob_bytes: count(bytes)?,
            blob_inventory_complete: complete,
            reserved_token_units: bounded(reserved),
            token_unit_limit: count(TOKEN_UNITS)?,
            tool_limit: count(TOOL_CALLS as u128)?,
            blob_byte_limit: count(BLOB_BYTES)?,
            overdrawn,
            can_start_model: !overdrawn
                && reserved + reserve <= TOKEN_UNITS
                && bytes + reserve <= BLOB_BYTES,
            can_dispatch: !overdrawn
                && bytes + u128::from(seed.limits.output_bytes.value()) + 65536 <= BLOB_BYTES,
        })
    }
}

pub(super) struct Measurement<'a> {
    pub id: &'a DomainId,
    pub invocation: Option<&'a DomainId>,
    pub meter: &'static str,
    pub source: &'a DomainId,
    pub unit: &'static str,
    pub quantity: Option<u64>,
    pub price: &'a DomainId,
}
pub(super) async fn record_usage(
    conn: &mut SqliteConnection,
    a: &LocalExecutionAccess<'_>,
    seed: &avencrew_contracts::wire::RpcRequestHarnessStartTurnBody,
    m: Measurement<'_>,
) -> Result<(), TurnError> {
    let quantity = m
        .quantity
        .and_then(|n| n.checked_mul(1_000_000))
        .and_then(|n| i64::try_from(n).ok());
    sqlx::query("INSERT INTO local_usage (id,workspace_id,created_at_us,run_id,attempt_id,invocation_id,meter,source_key,unit,quantity_micro,price_manifest_blob_id,cost_micro,quality,occurred_at_us) VALUES (?,?,?,?,?,?,?,?,?,?,?,0,?,?)")
        .bind(m.id.as_str()).bind(a.workspace.as_str()).bind(a.now.unix_micros()).bind(seed.run_spec.run_id.as_str()).bind(seed.lease.attempt_id.as_str()).bind(m.invocation.map(DomainId::as_str)).bind(m.meter).bind(m.source.as_str()).bind(m.unit).bind(quantity).bind(m.price.as_str()).bind(if quantity.is_some(){"actual"}else{"unknown"}).bind(a.now.unix_micros()).execute(conn).await?;
    Ok(())
}
