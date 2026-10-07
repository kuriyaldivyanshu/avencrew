//! Exact consumed catalog fields. This profile creates private, human-owned
//! first versions; future review/execution/change-objective paths fail closed.
use super::super::blobs::{canonical, digest, hex, invalid};
use super::super::identity::VerifiedIdentity;
use super::super::StoreError;
use avencrew_contracts::{
    bundles::RecordCandidate,
    scalars::{Counter, Digest, DomainId, Instant, Nullable, PositiveCounter, Text},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
type Label = Text<0, 16384>;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Resource {
    pub id: DomainId,
    pub workspace_id: DomainId,
    pub created_at: Instant,
    pub row_version: PositiveCounter,
    pub updated_at: Instant,
    pub project_id: Nullable<DomainId>,
    pub owner_id: Nullable<DomainId>,
    pub kind: String,
    pub visibility: String,
    pub status: String,
    pub acl_generation: PositiveCounter,
    pub current_version_id: Nullable<DomainId>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResourceVersion {
    pub id: DomainId,
    pub workspace_id: DomainId,
    pub created_at: Instant,
    pub resource_id: DomainId,
    pub version_no: PositiveCounter,
    pub blob_id: Nullable<DomainId>,
    pub inline_payload: Nullable<Value>,
    pub content_digest: Digest,
    pub observed_at: Instant,
    pub effective_at: Nullable<Instant>,
    pub fresh_until: Nullable<Instant>,
    pub source_etag: Nullable<Label>,
    pub created_by_id: DomainId,
    pub availability: String,
    pub erased_at: Nullable<Instant>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Blob {
    pub id: DomainId,
    pub workspace_id: DomainId,
    pub created_at: Instant,
    pub row_version: PositiveCounter,
    pub updated_at: Instant,
    pub owner_resource_id: DomainId,
    pub storage_key: Label,
    pub sha256: Digest,
    pub byte_size: Counter,
    pub media_type: Text<1, 16384>,
    pub encryption_key_ref: Nullable<Label>,
    pub status: String,
    pub verified_at: Nullable<Instant>,
    pub erased_at: Nullable<Instant>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Task {
    pub id: DomainId,
    pub workspace_id: DomainId,
    pub created_at: Instant,
    pub row_version: PositiveCounter,
    pub updated_at: Instant,
    pub resource_id: DomainId,
    pub owner_id: DomainId,
    pub current_revision_id: Nullable<DomainId>,
    pub status: String,
    pub priority: i32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TaskRevision {
    pub id: DomainId,
    pub workspace_id: DomainId,
    pub created_at: Instant,
    pub task_id: DomainId,
    pub version_no: i32,
    pub objective_version_id: DomainId,
    pub acting_principal_id: DomainId,
    pub audience_policy_id: DomainId,
    pub execution_policy_id: DomainId,
    pub completion_mode: String,
    pub accepted_by_id: DomainId,
    pub accepted_at: Instant,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TaskCheck {
    pub id: DomainId,
    pub workspace_id: DomainId,
    pub created_at: Instant,
    pub task_revision_id: DomainId,
    pub check_id: DomainId,
    pub required: bool,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CheckSpec {
    pub schema_version: i32,
    pub success_criteria: Text<1, 16384>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Check {
    pub id: DomainId,
    pub workspace_id: DomainId,
    pub created_at: Instant,
    pub resource_id: DomainId,
    pub version_no: i32,
    pub kind: String,
    pub spec: CheckSpec,
    pub digest: Digest,
    pub required: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Artifact {
    pub id: DomainId,
    pub workspace_id: DomainId,
    pub created_at: Instant,
    pub row_version: PositiveCounter,
    pub updated_at: Instant,
    pub resource_id: DomainId,
    pub artifact_kind: String,
    pub current_artifact_version_id: Nullable<DomainId>,
    pub status: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ArtifactVersion {
    pub id: DomainId,
    pub workspace_id: DomainId,
    pub created_at: Instant,
    pub artifact_id: DomainId,
    pub resource_version_id: DomainId,
    pub base_version_id: Nullable<DomainId>,
    pub producing_run_id: Nullable<DomainId>,
    pub manifest_id: Nullable<DomainId>,
    pub editor_id: DomainId,
}

pub(super) enum Record {
    Resource(Resource),
    ResourceVersion(ResourceVersion),
    Blob(Blob),
    Task(Task),
    TaskRevision(TaskRevision),
    TaskCheck(TaskCheck),
    Check(Check),
    Artifact(Artifact),
    ArtifactVersion(ArtifactVersion),
    Authority,
}
impl Record {
    pub fn decode(c: &RecordCandidate, identity: &VerifiedIdentity) -> Result<Self, StoreError> {
        if matches!(c.table.as_str(), "principals" | "policy_versions") {
            if !matches!(
                c.record_schema.as_str(),
                "server-v1:S0" | "server-v1:target"
            ) || identity.records.get(&(c.table.clone(), c.id.clone())) != Some(&c.record)
            {
                return Err(invalid(
                    "bundle cannot create or replace registered authority",
                ));
            }
            return Ok(Self::Authority);
        }
        if (c.table == "tasks" && c.record_schema != "server-v1:S0")
            || (c.table != "tasks"
                && !matches!(
                    c.record_schema.as_str(),
                    "server-v1:S0" | "server-v1:target"
                ))
        {
            return Err(invalid("unsupported record slice"));
        }
        macro_rules! typed {
            ($ty:ident) => {
                Self::$ty(
                    serde_json::from_value(c.record.clone())
                        .map_err(|_| invalid("invalid typed bundle record"))?,
                )
            };
        }
        let record = match c.table.as_str() {
            "resources" => typed!(Resource),
            "resource_versions" => typed!(ResourceVersion),
            "blobs" => typed!(Blob),
            "tasks" => typed!(Task),
            "task_revisions" => typed!(TaskRevision),
            "task_checks" => typed!(TaskCheck),
            "checks" => typed!(Check),
            "artifacts" => typed!(Artifact),
            "artifact_versions" => typed!(ArtifactVersion),
            _ => {
                return Err(invalid(
                    "unsupported record kind for foundation publication",
                ))
            }
        };
        record.validate(identity)?;
        Ok(record)
    }
    pub fn validate(&self, i: &VerifiedIdentity) -> Result<(), StoreError> {
        match self {
            Self::Resource(r) => {
                if r.row_version.value() != 1
                    || r.updated_at != r.created_at
                    || r.acl_generation.value() != 1
                    || r.project_id.0.is_some()
                    || r.owner_id.0.as_ref() != Some(&i.principal)
                    || r.visibility != "private"
                    || r.status != "active"
                    || !matches!(r.kind.as_str(), "task" | "artifact" | "document")
                    || r.current_version_id.0.is_none()
                {
                    return Err(invalid(
                        "resource widens scope or is not an initial private resource",
                    ));
                }
            }
            Self::ResourceVersion(r) => {
                if r.version_no.value() != 1
                    || r.created_by_id != i.principal
                    || r.availability != "present"
                    || r.erased_at.0.is_some()
                    || r.source_etag.0.is_some()
                    || r.fresh_until.0.is_some()
                    || r.observed_at.unix_micros() < r.created_at.unix_micros()
                    || r.blob_id.0.is_some() == r.inline_payload.0.is_some()
                {
                    return Err(invalid("unsupported or unavailable resource version"));
                }
                if let Some(body) = &r.inline_payload.0 {
                    let bytes = canonical(body)?;
                    if bytes.len() > 16_384 || hex(&digest(&bytes)) != r.content_digest.as_str() {
                        return Err(invalid("inline resource digest or size mismatch"));
                    }
                }
                let _ = &r.effective_at;
            }
            Self::Blob(r) => {
                if r.row_version.value() != 1
                    || r.updated_at != r.created_at
                    || r.status != "verified"
                    || r.verified_at.0.is_none()
                    || r.erased_at.0.is_some()
                    || r.encryption_key_ref.0.is_some()
                    || r.media_type
                        .as_str()
                        .starts_with("application/vnd.avencrew.")
                    || r.storage_key.as_str().is_empty()
                    || r.storage_key.as_str().starts_with('/')
                    || r.storage_key.as_str().contains('\0')
                    || r.storage_key.as_str().contains('\\')
                    || r.storage_key
                        .as_str()
                        .split('/')
                        .any(|part| matches!(part, ".." | "."))
                {
                    return Err(invalid("unsupported or unavailable content blob"));
                }
            }
            Self::Task(r) => {
                if r.owner_id != i.principal
                    || r.row_version.value() != 1
                    || r.updated_at != r.created_at
                    || r.status != "open"
                    || r.current_revision_id.0.is_none()
                {
                    return Err(invalid("task mutation requires the accepted-command path"));
                }
                let _ = r.priority;
            }
            Self::TaskRevision(r) => {
                if r.version_no != 1
                    || r.acting_principal_id != i.principal
                    || r.accepted_by_id != i.principal
                    || r.audience_policy_id != i.audience
                    || r.execution_policy_id != i.execution
                    || !matches!(r.completion_mode.as_str(), "bounded" | "ongoing")
                    || r.accepted_at.unix_micros() < r.created_at.unix_micros()
                {
                    return Err(invalid("task revision authority or version mismatch"));
                }
            }
            Self::TaskCheck(r) => {
                let _ = &r.created_at;
            }
            Self::Check(r) => {
                if r.version_no != 1
                    || r.kind != "human"
                    || r.spec.schema_version != 1
                    || hex(&digest(&canonical(&r.spec)?)) != r.digest.as_str()
                {
                    return Err(invalid("unsupported check specification or digest"));
                }
                let _ = &r.created_at;
            }
            Self::Artifact(r) => {
                if r.row_version.value() != 1
                    || r.updated_at != r.created_at
                    || r.status != "draft"
                    || r.current_artifact_version_id.0.is_none()
                    || !matches!(
                        r.artifact_kind.as_str(),
                        "report" | "document" | "patch" | "bundle" | "image" | "dataset" | "other"
                    )
                {
                    return Err(invalid("artifact review/edit path is not enabled"));
                }
            }
            Self::ArtifactVersion(r) => {
                if r.editor_id != i.principal
                    || r.base_version_id.0.is_some()
                    || r.producing_run_id.0.is_some()
                    || r.manifest_id.0.is_some()
                {
                    return Err(invalid(
                        "artifact producer/base/manifest feature is not enabled",
                    ));
                }
                let _ = &r.created_at;
            }
            Self::Authority => {}
        };
        Ok(())
    }
    pub fn identity(&self) -> Option<(&DomainId, &DomainId, &Instant)> {
        macro_rules! keys {
            ($r:expr) => {
                Some((&$r.id, &$r.workspace_id, &$r.created_at))
            };
        }
        match self {
            Self::Resource(r) => keys!(r),
            Self::ResourceVersion(r) => keys!(r),
            Self::Blob(r) => keys!(r),
            Self::Task(r) => keys!(r),
            Self::TaskRevision(r) => keys!(r),
            Self::TaskCheck(r) => keys!(r),
            Self::Check(r) => keys!(r),
            Self::Artifact(r) => keys!(r),
            Self::ArtifactVersion(r) => keys!(r),
            Self::Authority => None,
        }
    }
}
