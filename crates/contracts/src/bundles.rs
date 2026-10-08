//! Non-authoritative canonical bundle inspection. Publication belongs to the store.
//! A candidate is never a validated record closure or an identity registration.
use crate::{
    canonical_bytes,
    scalars::{Counter, Digest, DomainId},
    ContractError,
};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;

pub const MAX_RECORDS: usize = 10_000;
pub const MAX_BLOB_REFERENCES: usize = 10_000;

/// Purposes admitted for foundation inspection. Execution/catalog/wait/transfer
/// purposes remain unsupported until their owning feature has a validator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    Task,
    Artifact,
    Enrollment,
    Checkpoint,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordKey {
    pub table: String,
    pub id: DomainId,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordCandidate {
    pub table: String,
    pub id: DomainId,
    pub record_schema: String,
    /// Untrusted contents: a store-owned typed decoder must validate every field.
    pub record: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlobReference {
    pub id: DomainId,
    pub sha256: Digest,
    pub byte_size: Counter,
}
impl BlobReference {
    /// Checks supplied raw content, without resolving paths or granting access.
    /// Callers must bound file reads and authorize access before supplying bytes.
    pub fn verify(&self, bytes: &[u8]) -> Result<(), BundleError> {
        if bytes.len() as u64 != self.byte_size.value() {
            return Err(BundleError::BlobMismatch);
        }
        let actual = Sha256::digest(bytes);
        let expected = self.sha256.as_str().as_bytes();
        const HEX: &[u8; 16] = b"0123456789abcdef";
        if actual.iter().enumerate().any(|(i, b)| {
            expected[i * 2] != HEX[(b >> 4) as usize]
                || expected[i * 2 + 1] != HEX[(b & 15) as usize]
        }) {
            return Err(BundleError::BlobMismatch);
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    schema_version: String,
    purpose: Purpose,
    root: RecordKey,
    records: Vec<RecordCandidate>,
    referenced_blobs: Vec<BlobReference>,
}

/// Structurally inspected bytes only. No public constructor bypasses inspection.
/// Registry, record shape, closure, policy and publication checks remain required.
pub struct BundleCandidate {
    envelope: Envelope,
    canonical: Vec<u8>,
    digest: [u8; 32],
}
impl BundleCandidate {
    pub fn inspect(bytes: &[u8], workspace: &DomainId) -> Result<Self, BundleError> {
        // Reuses the duplicate-key, UTF-8, recursion, number and 1 MiB bounds
        // already enforced by the canonical contract serializer.
        let canonical = canonical_bytes(bytes)?;
        let envelope: Envelope =
            serde_json::from_slice(&canonical).map_err(|_| BundleError::Shape)?;
        if envelope.schema_version != "1.0" {
            return Err(BundleError::Version);
        }
        if envelope.records.is_empty()
            || envelope.records.len() > MAX_RECORDS
            || envelope.referenced_blobs.len() > MAX_BLOB_REFERENCES
        {
            return Err(BundleError::Limit);
        }
        if !allowed_root(envelope.purpose, &envelope.root.table) {
            return Err(BundleError::Purpose);
        }
        let mut records = BTreeMap::new();
        for record in &envelope.records {
            if !allowed_record(envelope.purpose, &record.table, &record.record_schema) {
                return Err(BundleError::Purpose);
            }
            let fields = record.record.as_object().ok_or(BundleError::Shape)?;
            if fields.get("id").and_then(Value::as_str) != Some(record.id.as_str()) {
                return Err(BundleError::Identity);
            }
            if record.table == "workspaces" {
                if record.id != *workspace {
                    return Err(BundleError::Workspace);
                }
            } else if fields.get("workspace_id").and_then(Value::as_str) != Some(workspace.as_str())
            {
                return Err(BundleError::Workspace);
            }
            let key = (&record.table, &record.id);
            if records
                .insert(key, record)
                .is_some_and(|previous| previous != record)
            {
                return Err(BundleError::Duplicate);
            }
        }
        if !records.contains_key(&(&envelope.root.table, &envelope.root.id)) {
            return Err(BundleError::MissingRoot);
        }
        let mut blobs = BTreeMap::new();
        for blob in &envelope.referenced_blobs {
            if blobs
                .insert(&blob.id, blob)
                .is_some_and(|previous| previous != blob)
            {
                return Err(BundleError::Duplicate);
            }
        }
        let digest = Sha256::digest(&canonical).into();
        Ok(Self {
            envelope,
            canonical,
            digest,
        })
    }
    pub fn purpose(&self) -> Purpose {
        self.envelope.purpose
    }
    pub fn root(&self) -> &RecordKey {
        &self.envelope.root
    }
    pub fn records(&self) -> &[RecordCandidate] {
        &self.envelope.records
    }
    pub fn referenced_blobs(&self) -> &[BlobReference] {
        &self.envelope.referenced_blobs
    }
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical
    }
    pub fn sha256(&self) -> [u8; 32] {
        self.digest
    }
    pub fn verify_digest(&self, expected: &[u8; 32]) -> Result<(), BundleError> {
        if &self.digest == expected {
            Ok(())
        } else {
            Err(BundleError::BlobMismatch)
        }
    }
}

/// Payload-free errors: neither hostile JSON nor identity metadata is echoed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleError {
    Json(ContractError),
    Shape,
    Version,
    Limit,
    Purpose,
    Identity,
    Workspace,
    Duplicate,
    MissingRoot,
    BlobMismatch,
}
impl From<ContractError> for BundleError {
    fn from(e: ContractError) -> Self {
        Self::Json(e)
    }
}
impl std::fmt::Display for BundleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bundle rejected: {self:?}")
    }
}
impl std::error::Error for BundleError {}

// Exact foundation purpose/table/schema labels from the canonical bundle design.
// This is an envelope allowlist, NOT a substitute for typed record validation.
fn allowed_root(purpose: Purpose, table: &str) -> bool {
    match purpose {
        Purpose::Task => matches!(
            table,
            "resources"
                | "resource_versions"
                | "blobs"
                | "tasks"
                | "task_revisions"
                | "task_checks"
                | "checks"
                | "check_results"
        ),
        Purpose::Artifact => matches!(
            table,
            "resources"
                | "resource_versions"
                | "blobs"
                | "artifacts"
                | "artifact_versions"
                | "checks"
                | "check_results"
        ),
        Purpose::Enrollment => matches!(table, "workspaces" | "principals" | "devices"),
        Purpose::Checkpoint => table == "checkpoints",
    }
}
fn allowed_record(purpose: Purpose, table: &str, schema: &str) -> bool {
    let allowed = match (purpose, table) {
        (Purpose::Checkpoint, "checkpoints") => return schema == "server-v1:S0",
        (Purpose::Checkpoint, "manifests" | "manifest_entries") => {
            return schema == "server-v1:target"
        }
        (Purpose::Task | Purpose::Artifact, "policy_versions") => {
            return schema == "server-v1:target"
        }
        (
            Purpose::Task,
            "resources" | "resource_versions" | "blobs" | "tasks" | "task_revisions"
            | "task_checks" | "checks" | "check_results" | "principals",
        ) => true,
        (
            Purpose::Artifact,
            "resources" | "resource_versions" | "blobs" | "artifacts" | "artifact_versions"
            | "checks" | "check_results" | "principals",
        ) => true,
        (Purpose::Enrollment, "workspaces" | "principals" | "devices") => true,
        _ => false,
    };
    allowed
        && matches!(
            schema,
            "server-v1:S0"
                | "server-v1:S1"
                | "server-v1:S2"
                | "server-v1:S3"
                | "server-v1:S4"
                | "server-v1:S5"
                | "server-v1:S6"
                | "server-v1:target"
        )
}
