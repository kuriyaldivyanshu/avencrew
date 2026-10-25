//! Fixture launch preparation uses the existing identity/task/run publications.
use super::*;
use avencrew_contracts::scalars::PositiveCounter;
use avencrew_store_sqlite::{
    BundlePublication, LocalRunRegistration, LocalTurnPublication, RecordAllocation,
};
use sha2_auth::{Digest as _, Sha256};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureLaunch {
    pub schema_version: String,
    pub registration: Value,
    pub task_bundle: Value,
    pub profile: FixtureProfile,
}
pub struct PreparedFixtureTurn {
    pub workspace: DomainId,
    pub actor: DomainId,
    pub turn: DomainId,
}

pub async fn prepare_fixture_turn(
    store: &mut LocalStore,
    launch: &FixtureLaunch,
) -> Result<PreparedFixtureTurn, EngineError> {
    prepare_fixture_turn_with_limits(store, launch, 10, 60_000).await
}

// Tests publish a real seed through the production store without changing any
// launch defaults or adding a model/user configuration surface.
#[cfg(test)]
pub(super) async fn prepare_fixture_turn_for_test(
    store: &mut LocalStore,
    launch: &FixtureLaunch,
    steps: u32,
    wall_ms: u32,
) -> Result<PreparedFixtureTurn, EngineError> {
    prepare_fixture_turn_with_limits(store, launch, steps, wall_ms).await
}

async fn prepare_fixture_turn_with_limits(
    store: &mut LocalStore,
    launch: &FixtureLaunch,
    steps: u32,
    wall_ms: u32,
) -> Result<PreparedFixtureTurn, EngineError> {
    if launch.schema_version != "avencrew.fixture-launch/1"
        || launch.profile.provider != "fixture"
        || launch.profile.rate_micro != "0"
    {
        return Err(EngineError::InvalidState);
    }
    let read_id = |v: &Value| {
        serde_json::from_value::<DomainId>(v.clone()).map_err(|_| EngineError::InvalidState)
    };
    let registration = store
        .bootstrap_standalone(
            &serde_json::to_vec(&launch.registration).map_err(|_| EngineError::InvalidState)?,
            "fixture:synthetic-no-secret",
        )
        .await
        .map_err(|error| {
            #[cfg(test)]
            eprintln!("fixture preparation: {error}");
            let _ = error;
            EngineError::Storage
        })?;
    let workspace = registration.workspace_id;
    let actor = registration.actor_id;
    let bundle = avencrew_contracts::canonical_bytes(
        &serde_json::to_vec(&launch.task_bundle).map_err(|_| EngineError::InvalidState)?,
    )
    .map_err(|_| EngineError::InvalidState)?;
    let records = launch.task_bundle["records"]
        .as_array()
        .ok_or(EngineError::InvalidState)?;
    let allocations = records
        .iter()
        .filter(|r| !matches!(r["table"].as_str(), Some("principals" | "policy_versions")))
        .map(|r| {
            Ok(RecordAllocation {
                table: r["table"].as_str().ok_or(EngineError::InvalidState)?.into(),
                id: read_id(&r["id"])?,
                blob_id: id(),
            })
        })
        .collect::<Result<Vec<_>, EngineError>>()?;
    let receipt = store
        .publish_bundle(BundlePublication {
            workspace: &workspace,
            actor: &actor,
            bundle_blob_id: &id(),
            bytes: &bundle,
            sha256: Sha256::digest(&bundle).into(),
            record_blobs: &allocations,
            payloads: &[],
        })
        .await
        .map_err(|error| {
            #[cfg(test)]
            eprintln!("fixture preparation: {error}");
            let _ = error;
            EngineError::Storage
        })?;
    let revision = records
        .iter()
        .find(|r| r["table"] == "task_revisions" && r["id"] == receipt.version_id.as_str())
        .map(|r| &r["record"])
        .ok_or(EngineError::InvalidState)?;
    let sample = clock::sample().map_err(|_| EngineError::StaleAuthority)?;
    let run = id();
    let turn = id();
    store
        .register_local_run(LocalRunRegistration {
            workspace: &workspace,
            actor: &actor,
            task: &receipt.root_id,
            task_revision: &receipt.version_id,
            run: &run,
            event: &id(),
            queue: &id(),
            payload_blob: &id(),
            at: &sample.now,
        })
        .await
        .map_err(|error| {
            #[cfg(test)]
            eprintln!("fixture preparation: {error}");
            let _ = error;
            EngineError::Storage
        })?;
    let expires = chrono::DateTime::from_timestamp_micros(sample.now.unix_micros() + 600_000_000)
        .ok_or(EngineError::InvalidState)?
        .format("%Y-%m-%dT%H:%M:%S%.6fZ")
        .to_string();
    let checks = records
        .iter()
        .filter(|r| r["table"] == "task_checks")
        .map(|r| r["record"]["check_id"].clone())
        .collect::<Vec<_>>();
    let principal = &launch.registration["registration"]["principal_id"];
    let task_record = records
        .iter()
        .find(|r| r["table"] == "tasks")
        .map(|r| &r["record"])
        .ok_or(EngineError::InvalidState)?;
    let seed:TurnSeed=serde_json::from_value(json!({"run_spec":{"task_id":task_record["id"],"task_revision_id":receipt.version_id,"run_id":run,"session_id":id(),"branch_id":id(),"workspace_id":workspace,"project_id":null,"worker_id":null,"responsibility_id":null,"requester_id":principal,"owner_id":principal,"acting_principal_id":principal,"audience_policy_id":revision["audience_policy_id"],"execution_policy_id":revision["execution_policy_id"],"objective_version_id":revision["objective_version_id"],"check_refs":checks,"instruction_digests":[],"model_profile_id":id(),"environment_recipe_version_id":null,"budget_period_refs":[],"deadline":null,"placement":"local","accepted_input_seq":"0"},"lease":{"run_id":run,"attempt_id":id(),"epoch":"1","placement":"local","authority":"local","environment_id":null,"generation":null,"expires_at":expires,"boot_id":sample.boot,"monotonic_deadline_ns":(sample.monotonic.value()+600_000_000_000).to_string(),"receipt_digest":hash(b"fixture-lease/1")},"turn_id":turn,"accepted_input_cut":"0","checkpoint_id":null,"limits":{"model_steps":steps,"active_wall_ms":wall_ms,"output_bytes":32768}})).map_err(|_|EngineError::InvalidState)?;
    store
        .publish_local_turn(LocalTurnPublication {
            actor: &actor,
            seed: &seed,
            ordinal: &PositiveCounter::new("1").map_err(|_| EngineError::InvalidState)?,
            expected_run_version: &PositiveCounter::new("1")
                .map_err(|_| EngineError::InvalidState)?,
            spec_blob: &id(),
            lease_blob: &id(),
            event: &id(),
            harness_build_digest: &hash(b"avencrew.fixture-supervisor/1"),
            at: &sample.now,
            now: &sample.now,
            current_boot: &sample.boot,
            monotonic_now: &sample.monotonic,
        })
        .await
        .map_err(storage)?;
    Ok(PreparedFixtureTurn {
        workspace,
        actor,
        turn,
    })
}
