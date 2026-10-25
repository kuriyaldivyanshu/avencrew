//! Exact private candidate versions and version-bound checks. No review acceptance.
use super::*;
use avencrew_contracts::{
    runtime::{ArtifactFormat, ArtifactValidation},
    wire::ArtifactRef,
};

const MEDIA: &str = "application/vnd.avencrew.artifact-checks+json";
pub enum LocalArtifactTarget<'a> {
    Candidate {
        step: &'a DomainId,
        artifact: &'a DomainId,
    },
    Edit {
        artifact: &'a DomainId,
        expected_version: &'a DomainId,
    },
}
pub struct LocalArtifactPublication<'a> {
    pub access: LocalExecutionAccess<'a>,
    pub target: LocalArtifactTarget<'a>,
    pub version: &'a DomainId,
    pub content_blob: &'a DomainId,
    pub checks_manifest: &'a DomainId,
    pub checks_blob: &'a DomainId,
    pub result: &'a DomainId,
    pub bytes: &'a [u8],
    pub validation: &'a ArtifactValidation,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalArtifactReceipt {
    pub artifact: ArtifactRef,
    pub content_blob_id: DomainId,
    pub checks_manifest_id: DomainId,
    pub check_result_id: DomainId,
}
pub struct LocalArtifact {
    pub receipt: LocalArtifactReceipt,
    pub validation: ArtifactValidation,
    pub pending_checks: Vec<DomainId>,
    pub audience_policy_id: DomainId,
    pub file: std::fs::File,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Checks {
    schema_version: String,
    receipt: LocalArtifactReceipt,
    workspace_id: DomainId,
    actor_id: DomainId,
    task_id: DomainId,
    objective_version_id: DomainId,
    audience_policy_id: DomainId,
    source_context_blob_id: DomainId,
    validation: ArtifactValidation,
    pending_checks: Vec<DomainId>,
    review_status: String,
}
fn kind(format: ArtifactFormat) -> &'static str {
    match format {
        ArtifactFormat::Text => "text",
        ArtifactFormat::Json => "json",
        ArtifactFormat::Csv => "csv",
        ArtifactFormat::Patch => "patch",
    }
}
// Human edits are permitted after the candidate outcome enters verifying.
// They still require this turn's current, unexpired local authority and no stop.
async fn check_edit_cut(
    conn: &mut SqliteConnection,
    a: &LocalExecutionAccess<'_>,
    seed: &avencrew_contracts::wire::RpcRequestHarnessStartTurnBody,
    device: &DomainId,
) -> Result<(), TurnError> {
    check_lease_cut(a, seed)?;
    let row = run_row(conn, a.workspace, &seed.run_spec.run_id, a.actor).await?;
    require_local(&row, device)?;
    if row.cancelled {
        return Err(TurnError::Cancelled);
    }
    if row.epoch != seed.lease.epoch.value() as i64
        || row.task != seed.run_spec.task_id
        || row.revision != seed.run_spec.task_revision_id
        || row.accepted != row.applied
        || !matches!(
            row.status.as_str(),
            "preparing" | "running" | "paused" | "verifying"
        )
    {
        return Err(TurnError::StaleAuthority);
    }
    Ok(())
}
impl LocalStore {
    pub async fn publish_local_artifact(
        &mut self,
        p: LocalArtifactPublication<'_>,
    ) -> Result<LocalArtifactReceipt, TurnError> {
        let _proof_scope = self.proof_read_scope();
        let a = &p.access;
        let retained = self.read_local_turn(a.workspace, a.actor, a.turn).await?;
        let identity = self.verified_identity(a.workspace, a.actor).await?;
        if matches!(&p.target, LocalArtifactTarget::Edit { .. }) {
            check_edit_cut(&mut self.connection, a, &retained.seed, identity.device()).await?;
        }
        let (artifact, base, step, ordinal, source) = match &p.target {
            LocalArtifactTarget::Candidate { step, artifact } => {
                let seed = self.authorize_local_execution(a, false).await?;
                let snap = self
                    .local_execution_snapshot(a.workspace, a.actor, a.turn)
                    .await?;
                if snap.progress.pending_step_id.as_ref() != Some(*step)
                    || !matches!(snap.model.as_ref(),Some(Ok(r)) if r.response.finish==NormalizedModelResponseFinish::Final)
                {
                    return Err(TurnError::Conflict);
                }
                let sealed = self
                    .read_local_step(a.workspace, a.actor, a.turn, step)
                    .await?;
                let allocation = sealed.candidate.as_ref().ok_or(TurnError::Unsupported)?;
                if allocation.artifact_id != **artifact
                    || allocation.version_id != *p.version
                    || allocation.content_blob_id != *p.content_blob
                    || allocation.checks_manifest_id != *p.checks_manifest
                    || allocation.checks_blob_id != *p.checks_blob
                    || allocation.check_result_id != *p.result
                {
                    return Err(TurnError::IdempotencyMismatch);
                }
                if let Some(old_row)=sqlx::query("SELECT id,artifact_id FROM local_artifact_versions WHERE workspace_id=? AND candidate_step_id=?").bind(a.workspace.as_str()).bind(step.as_str()).fetch_optional(&mut self.connection).await?{
                    let version=DomainId::new(old_row.try_get::<String,_>("id")?).map_err(|_|TurnError::Conflict)?;
                    let old_artifact=DomainId::new(old_row.try_get::<String,_>("artifact_id")?).map_err(|_|TurnError::Conflict)?;
                    let old=self.read_local_artifact(a.workspace,a.actor,&old_artifact,&version).await?;
                    if old.validation!=*p.validation||old.receipt.artifact.sha256!=p.validation.sha256{return Err(TurnError::IdempotencyMismatch);}return Ok(old.receipt);
                }

                if sealed.context.run_id != seed.run_spec.run_id
                    || sealed.accepted_input_cut != snap.progress.state.applied_input_cut
                {
                    return Err(TurnError::Conflict);
                }
                let completion = snap
                    .model
                    .as_ref()
                    .and_then(|r| r.as_ref().ok())
                    .ok_or(TurnError::Conflict)?;
                let text = completion
                    .response
                    .items
                    .as_slice()
                    .iter()
                    .filter_map(|item| match item {
                        avencrew_contracts::wire::NormalizedModelResponseItemsItem::Text {
                            text,
                        } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                if text.as_bytes() != p.bytes {
                    return Err(TurnError::InvalidSeed);
                }
                (
                    (*artifact).clone(),
                    None,
                    Some((*step).clone()),
                    1i64,
                    sealed.binding.context_manifest_id,
                )
            }
            LocalArtifactTarget::Edit {
                artifact,
                expected_version,
            } => {
                let old = self
                    .read_local_artifact(a.workspace, a.actor, artifact, expected_version)
                    .await?;
                if old.validation.format != p.validation.format {
                    return Err(TurnError::Unsupported);
                }
                let current: String = sqlx::query_scalar(
                    "SELECT current_version_id FROM local_artifacts WHERE workspace_id=? AND id=?",
                )
                .bind(a.workspace.as_str())
                .bind(artifact.as_str())
                .fetch_one(&mut self.connection)
                .await?;
                if current != expected_version.as_str() {
                    return Err(TurnError::Conflict);
                }
                let old_checks = self
                    .read_artifact_checks(a.workspace, a.actor, &old.receipt)
                    .await?;
                if old_checks.task_id != retained.seed.run_spec.task_id
                    || old_checks.objective_version_id
                        != retained.seed.run_spec.objective_version_id
                {
                    return Err(TurnError::Conflict);
                }
                let ordinal: i64 = sqlx::query_scalar(
                    "SELECT version_no FROM local_artifact_versions WHERE workspace_id=? AND id=?",
                )
                .bind(a.workspace.as_str())
                .bind(expected_version.as_str())
                .fetch_one(&mut self.connection)
                .await?;
                (
                    (*artifact).clone(),
                    Some((*expected_version).clone()),
                    None,
                    next(ordinal)?,
                    old_checks.source_context_blob_id,
                )
            }
        };
        if p.bytes.len() > 262144
            || p.validation.schema_version != "avencrew.artifact-validation/1"
            || p.validation.sha256.as_str() != hex(&digest(p.bytes))
            || p.validation.byte_size.value() != p.bytes.len() as u64
            || p.validation.codes.is_empty()
            || p.validation.codes.len() > 16
            || p.validation
                .codes
                .iter()
                .any(|c| c.is_empty() || c.len() > 128)
        {
            return Err(TurnError::InvalidSeed);
        }
        let receipt = LocalArtifactReceipt {
            artifact: ArtifactRef {
                artifact_id: artifact.clone(),
                version_id: p.version.clone(),
                sha256: p.validation.sha256.clone(),
                byte_size: p.validation.byte_size.clone(),
            },
            content_blob_id: p.content_blob.clone(),
            checks_manifest_id: p.checks_manifest.clone(),
            check_result_id: p.result.clone(),
        };
        let checks = Checks {
            schema_version: "avencrew.artifact-checks/2".into(),
            receipt: receipt.clone(),
            workspace_id: a.workspace.clone(),
            actor_id: a.actor.clone(),
            task_id: retained.seed.run_spec.task_id.clone(),
            objective_version_id: retained.seed.run_spec.objective_version_id.clone(),
            audience_policy_id: retained.seed.run_spec.audience_policy_id.clone(),
            source_context_blob_id: source,
            validation: p.validation.clone(),
            pending_checks: retained.seed.run_spec.check_refs.as_slice().to_vec(),
            review_status: "pending".into(),
        };
        let bytes = canonical(&checks)?;
        self.put_canonical(a.workspace, p.checks_blob, &bytes)?;
        self.import_vault_blob(
            crate::BlobImport {
                workspace: a.workspace,
                actor: a.actor,
                id: p.content_blob,
                sha256: &p.validation.sha256,
                byte_size: &p.validation.byte_size,
                at: a.now,
            },
            p.bytes,
        )
        .await?;
        let identity = self.verified_identity(a.workspace, a.actor).await?;
        let mut tx = self.connection.begin().await?;
        identity.check_live(&mut tx).await?;
        if step.is_some() {
            check_dispatch_cut(&mut tx, a, &retained.seed).await?;
        } else {
            check_edit_cut(&mut tx, a, &retained.seed, identity.device()).await?;
        }
        blob(
            &mut tx,
            a.workspace,
            a.actor,
            p.checks_blob,
            &bytes,
            MEDIA,
            a.now.unix_micros(),
        )
        .await?;
        sqlx::query("INSERT INTO local_manifests (id,workspace_id,created_at_us,owner_actor_id,kind,schema_version,canonical_blob_id,digest,sealed_at_us,deletion_generation) VALUES (?,?,?,?,'snapshot',1,?,?,?,0)")
            .bind(p.checks_manifest.as_str()).bind(a.workspace.as_str()).bind(a.now.unix_micros()).bind(a.actor.as_str()).bind(p.checks_blob.as_str()).bind(digest(&bytes).as_slice()).bind(a.now.unix_micros()).execute(&mut *tx).await?;
        if base.is_none() {
            sqlx::query("INSERT INTO local_artifacts (id,workspace_id,created_at_us,owner_actor_id,kind,current_version_id,status) VALUES (?,?,?,?,?,NULL,'draft')").bind(artifact.as_str()).bind(a.workspace.as_str()).bind(a.now.unix_micros()).bind(a.actor.as_str()).bind(kind(p.validation.format)).execute(&mut *tx).await?;
        }
        sqlx::query("INSERT INTO local_artifact_versions (id,workspace_id,created_at_us,artifact_id,base_version_id,run_id,content_blob_id,checks_manifest_id,version_no,candidate_step_id) VALUES (?,?,?,?,?,?,?,?,?,?)")
            .bind(p.version.as_str()).bind(a.workspace.as_str()).bind(a.now.unix_micros()).bind(artifact.as_str()).bind(base.as_ref().map(DomainId::as_str)).bind(step.as_ref().map(|_|retained.seed.run_spec.run_id.as_str())).bind(p.content_blob.as_str()).bind(p.checks_manifest.as_str()).bind(ordinal).bind(step.as_ref().map(DomainId::as_str)).execute(&mut *tx).await?;
        let changed=sqlx::query("UPDATE local_artifacts SET current_version_id=? WHERE workspace_id=? AND id=? AND current_version_id IS ? AND owner_actor_id=?")
            .bind(p.version.as_str()).bind(a.workspace.as_str()).bind(artifact.as_str()).bind(base.as_ref().map(DomainId::as_str)).bind(a.actor.as_str()).execute(&mut *tx).await?;
        if changed.rows_affected() != 1 {
            return Err(TurnError::Conflict);
        }
        tx.commit().await?;
        Ok(receipt)
    }

    pub(super) async fn read_artifact_checks(
        &mut self,
        w: &DomainId,
        actor: &DomainId,
        receipt: &LocalArtifactReceipt,
    ) -> Result<Checks, TurnError> {
        let _proof_scope = self.proof_read_scope();
        let row=sqlx::query("SELECT canonical_blob_id,digest,kind FROM local_manifests WHERE workspace_id=? AND id=? AND owner_actor_id=?").bind(w.as_str()).bind(receipt.checks_manifest_id.as_str()).bind(actor.as_str()).fetch_one(&mut self.connection).await?;
        let id = DomainId::new(row.try_get::<String, _>("canonical_blob_id")?)
            .map_err(|_| TurnError::Conflict)?;
        let bytes = self.read_registered_blob(w, actor, &id, MEDIA).await?;
        let checks: Checks = serde_json::from_slice(&bytes).map_err(|_| TurnError::Conflict)?;
        if canonical(&checks)? != bytes
            || checks.schema_version != "avencrew.artifact-checks/2"
            || checks.workspace_id != *w
            || checks.actor_id != *actor
            || checks.receipt != *receipt
            || checks.review_status != "pending"
            || row.try_get::<Vec<u8>, _>("digest")? != digest(&bytes)
            || row.try_get::<String, _>("kind")? != "snapshot"
        {
            return Err(TurnError::Conflict);
        }
        self.read_registered_blob(w, actor, &checks.source_context_blob_id, STEP_MEDIA)
            .await?;
        let source = sqlx::query(
            "SELECT id,turn_id FROM local_steps WHERE workspace_id=? AND context_blob_id=?",
        )
        .bind(w.as_str())
        .bind(checks.source_context_blob_id.as_str())
        .fetch_one(&mut self.connection)
        .await?;
        let turn = DomainId::new(source.try_get::<String, _>("turn_id")?)
            .map_err(|_| TurnError::Conflict)?;
        let step =
            DomainId::new(source.try_get::<String, _>("id")?).map_err(|_| TurnError::Conflict)?;
        self.read_local_step(w, actor, &turn, &step).await?;
        let retained = self.read_local_turn(w, actor, &turn).await?;
        let identity = self.verified_identity(w, actor).await?;
        if checks.task_id != retained.seed.run_spec.task_id
            || checks.objective_version_id != retained.seed.run_spec.objective_version_id
            || checks.audience_policy_id != retained.seed.run_spec.audience_policy_id
            || checks.audience_policy_id != identity.audience
            || checks.pending_checks != retained.seed.run_spec.check_refs.as_slice()
        {
            return Err(TurnError::Conflict);
        }
        Ok(checks)
    }

    pub async fn read_local_artifact(
        &mut self,
        w: &DomainId,
        actor: &DomainId,
        artifact: &DomainId,
        version: &DomainId,
    ) -> Result<LocalArtifact, TurnError> {
        let _proof_scope = self.proof_read_scope();
        self.verified_identity(w, actor).await?;
        let row=sqlx::query("SELECT v.content_blob_id,v.checks_manifest_id,a.kind,m.canonical_blob_id FROM local_artifact_versions v JOIN local_artifacts a ON a.workspace_id=v.workspace_id AND a.id=v.artifact_id JOIN local_manifests m ON m.workspace_id=v.workspace_id AND m.id=v.checks_manifest_id WHERE v.workspace_id=? AND v.id=? AND v.artifact_id=? AND a.owner_actor_id=?")
            .bind(w.as_str()).bind(version.as_str()).bind(artifact.as_str()).bind(actor.as_str()).fetch_one(&mut self.connection).await?;
        let checks_id = DomainId::new(row.try_get::<String, _>("canonical_blob_id")?)
            .map_err(|_| TurnError::Conflict)?;
        let bytes = self
            .read_registered_blob(w, actor, &checks_id, MEDIA)
            .await?;
        let checks: Checks = serde_json::from_slice(&bytes).map_err(|_| TurnError::Conflict)?;
        let receipt = checks.receipt.clone();
        if receipt.artifact.artifact_id != *artifact
            || receipt.artifact.version_id != *version
            || receipt.content_blob_id.as_str() != row.try_get::<String, _>("content_blob_id")?
            || receipt.checks_manifest_id.as_str()
                != row.try_get::<String, _>("checks_manifest_id")?
            || kind(checks.validation.format) != row.try_get::<String, _>("kind")?
        {
            return Err(TurnError::Conflict);
        }
        let checks = self.read_artifact_checks(w, actor, &receipt).await?;
        let task = self.read_task_bundle(w, actor, &checks.task_id).await?;
        let task: serde_json::Value =
            serde_json::from_slice(&task.canonical_bytes).map_err(|_| TurnError::Conflict)?;
        if !task["records"].as_array().is_some_and(|rs| {
            rs.iter().any(|r| {
                r["table"] == "resource_versions" && r["id"] == checks.objective_version_id.as_str()
            })
        }) {
            return Err(TurnError::Conflict);
        }
        let file = self
            .open_vault_blob(w, actor, &receipt.content_blob_id)
            .await?;
        let metadata =
            sqlx::query("SELECT sha256,byte_size FROM local_blobs WHERE workspace_id=? AND id=?")
                .bind(w.as_str())
                .bind(receipt.content_blob_id.as_str())
                .fetch_one(&mut self.connection)
                .await?;
        if metadata.try_get::<Vec<u8>, _>("sha256")?
            != digest_bytes(receipt.artifact.sha256.as_str())
            || metadata.try_get::<i64, _>("byte_size")? != receipt.artifact.byte_size.value() as i64
            || checks.validation.sha256 != receipt.artifact.sha256
            || checks.validation.byte_size != receipt.artifact.byte_size
        {
            return Err(TurnError::Conflict);
        }
        Ok(LocalArtifact {
            receipt,
            validation: checks.validation,
            pending_checks: checks.pending_checks,
            audience_policy_id: checks.audience_policy_id,
            file,
        })
    }
}
