//! First private task/artifact versions. Saving a bundle is not execution,
//! review acceptance or evidence that a business outcome is complete.
mod records;
use super::blobs::{canonical, digest, hex, invalid, relative, MAX_BYTES};
use super::identity::VerifiedIdentity;
use super::{LocalStore, StoreError};
use avencrew_contracts::{
    bundles::{BundleCandidate, Purpose, RecordCandidate, RecordKey},
    scalars::{Digest, DomainId},
};
use records::*;
use sqlx::{Connection, Row};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const BUNDLE_MEDIA: &str = "application/vnd.avencrew.record-bundle+json";
const INDEX_MEDIA: &str = "application/vnd.avencrew.bundle-record+json";
const MAX_CONTENT_BYTES: usize = 16 * MAX_BYTES;
type Key = (String, DomainId);
type BlobMetadata = (String, Vec<u8>, i64, String, String, String, Option<i64>);

pub struct RecordAllocation {
    pub table: String,
    pub id: DomainId,
    pub blob_id: DomainId,
}
pub struct Payload<'a> {
    pub id: &'a DomainId,
    pub bytes: &'a [u8],
}
/// Trusted caller allocates storage identities; never supplies projection fields
/// or a permission decision. The store verifies its own retained authority.
pub struct BundlePublication<'a> {
    pub workspace: &'a DomainId,
    pub actor: &'a DomainId,
    pub bundle_blob_id: &'a DomainId,
    pub bytes: &'a [u8],
    pub sha256: [u8; 32],
    pub record_blobs: &'a [RecordAllocation],
    pub payloads: &'a [Payload<'a>],
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicationReceipt {
    pub root_table: String,
    pub root_id: DomainId,
    pub version_id: DomainId,
    pub bundle_blob_id: DomainId,
    pub sha256: Digest,
}
pub struct PublishedBundle {
    pub receipt: PublicationReceipt,
    pub canonical_bytes: Vec<u8>,
}
struct ValidatedClosure {
    candidate: BundleCandidate,
    records: BTreeMap<Key, Record>,
    originals: BTreeMap<Key, RecordCandidate>,
    identity: VerifiedIdentity,
    version_id: DomainId,
}
struct StagedBlob {
    id: DomainId,
    path: String,
    bytes: Vec<u8>,
    media: String,
}
fn object_path(workspace: &DomainId, key: &Key) -> String {
    format!(
        "blobs/{}/records/{}/{}.json",
        workspace.as_str(),
        key.0,
        key.1.as_str()
    )
}
fn key(table: &str, id: &DomainId) -> Key {
    (table.to_owned(), id.clone())
}
fn required<'a>(
    records: &'a BTreeMap<Key, Record>,
    table: &str,
    id: &DomainId,
) -> Result<&'a Record, StoreError> {
    records
        .get(&key(table, id))
        .ok_or(invalid("missing typed bundle parent"))
}

impl ValidatedClosure {
    fn validate(
        candidate: BundleCandidate,
        identity: VerifiedIdentity,
    ) -> Result<Self, StoreError> {
        if !matches!(
            (candidate.purpose(), candidate.root().table.as_str()),
            (Purpose::Task, "tasks") | (Purpose::Artifact, "artifacts")
        ) {
            return Err(invalid("unsupported publication root"));
        }
        let mut records = BTreeMap::new();
        let mut originals = BTreeMap::new();
        for c in candidate.records() {
            records.insert(key(&c.table, &c.id), Record::decode(c, &identity)?);
            originals.insert(key(&c.table, &c.id), c.clone());
        }
        let root = key(&candidate.root().table, &candidate.root().id);
        let version_id = match required(&records, &root.0, &root.1)? {
            Record::Task(t) => t
                .current_revision_id
                .0
                .clone()
                .ok_or(invalid("missing task revision"))?,
            Record::Artifact(a) => a
                .current_artifact_version_id
                .0
                .clone()
                .ok_or(invalid("missing artifact version"))?,
            _ => return Err(invalid("wrong publication root type")),
        };
        let mut visited = BTreeSet::new();
        let mut queue = VecDeque::from([(root.clone(), 0usize)]);
        while let Some((node, depth)) = queue.pop_front() {
            if visited.contains(&node) {
                continue;
            }
            if depth > 128 {
                return Err(invalid("bundle closure traversal bound"));
            }
            visited.insert(node.clone());
            if identity.records.contains_key(&node) && !records.contains_key(&node) {
                continue;
            }
            let record = records
                .get(&node)
                .ok_or(invalid("missing typed bundle dependency"))?;
            let mut edges = Vec::new();
            macro_rules! edge {
                ($table:literal,$id:expr) => {
                    edges.push(key($table, $id))
                };
            }
            match record {
                Record::Resource(r) => {
                    edge!("principals", &identity.principal);
                    let version = r
                        .current_version_id
                        .0
                        .as_ref()
                        .ok_or(invalid("missing resource current version"))?;
                    if !matches!(required(&records,"resource_versions",version)?,Record::ResourceVersion(v) if v.resource_id==r.id)
                    {
                        return Err(invalid(
                            "resource current version belongs to another parent",
                        ));
                    }
                    edge!("resource_versions", version);
                    for ((table, id), check) in &records {
                        if candidate.purpose() == Purpose::Artifact
                            && matches!(check,Record::Check(c) if c.resource_id==r.id)
                        {
                            edges.push((table.clone(), id.clone()));
                        }
                    }
                }
                Record::ResourceVersion(v) => {
                    edge!("resources", &v.resource_id);
                    edge!("principals", &v.created_by_id);
                    if let Some(id) = &v.blob_id.0 {
                        if !matches!(required(&records,"blobs",id)?,Record::Blob(b) if b.owner_resource_id==v.resource_id && b.sha256==v.content_digest)
                        {
                            return Err(invalid("resource payload owner/digest mismatch"));
                        }
                        edge!("blobs", id);
                    }
                }
                Record::Blob(b) => {
                    edge!("resources", &b.owner_resource_id);
                }
                Record::Task(t) => {
                    if !matches!(required(&records,"resources",&t.resource_id)?,Record::Resource(r) if r.kind=="task")
                    {
                        return Err(invalid("task resource kind mismatch"));
                    }
                    let rev = t
                        .current_revision_id
                        .0
                        .as_ref()
                        .ok_or(invalid("missing task current revision"))?;
                    if !matches!(required(&records,"task_revisions",rev)?,Record::TaskRevision(r) if r.task_id==t.id)
                    {
                        return Err(invalid("task revision belongs to another task"));
                    }
                    edge!("resources", &t.resource_id);
                    edge!("principals", &t.owner_id);
                    edge!("task_revisions", rev);
                }
                Record::TaskRevision(r) => {
                    let task = match required(&records, "tasks", &r.task_id)? {
                        Record::Task(t) => t,
                        _ => return Err(invalid("task revision parent kind mismatch")),
                    };
                    if !matches!(required(&records,"resource_versions",&r.objective_version_id)?,Record::ResourceVersion(v) if v.resource_id==task.resource_id)
                    {
                        return Err(invalid(
                            "objective version belongs to another task resource",
                        ));
                    }
                    edge!("tasks", &r.task_id);
                    edge!("resource_versions", &r.objective_version_id);
                    edge!("principals", &r.acting_principal_id);
                    edge!("principals", &r.accepted_by_id);
                    edge!("policy_versions", &r.audience_policy_id);
                    edge!("policy_versions", &r.execution_policy_id);
                    let mut check_ids = BTreeSet::new();
                    let mut required_count = 0;
                    for ((table, id), c) in &records {
                        if let Record::TaskCheck(c) = c {
                            if c.task_revision_id == r.id {
                                if !check_ids.insert(&c.check_id) {
                                    return Err(invalid("duplicate revision/check link"));
                                }
                                if !matches!(required(&records,"checks",&c.check_id)?,Record::Check(check) if check.resource_id==task.resource_id && check.required==c.required)
                                {
                                    return Err(invalid(
                                        "task check target or required flag mismatch",
                                    ));
                                }
                                required_count += usize::from(c.required);
                                edges.push((table.clone(), id.clone()));
                            }
                        }
                    }
                    if required_count == 0 {
                        return Err(invalid("task has no required acceptance check"));
                    }
                }
                Record::TaskCheck(c) => {
                    edge!("task_revisions", &c.task_revision_id);
                    edge!("checks", &c.check_id);
                }
                Record::Check(c) => {
                    edge!("resources", &c.resource_id);
                }
                Record::Artifact(a) => {
                    if !matches!(required(&records,"resources",&a.resource_id)?,Record::Resource(r) if r.kind=="artifact")
                    {
                        return Err(invalid("artifact resource kind mismatch"));
                    }
                    let version = a
                        .current_artifact_version_id
                        .0
                        .as_ref()
                        .ok_or(invalid("missing artifact current version"))?;
                    if !matches!(required(&records,"artifact_versions",version)?,Record::ArtifactVersion(v) if v.artifact_id==a.id)
                    {
                        return Err(invalid("artifact version belongs to another artifact"));
                    }
                    // Preserve the exact selected audience policy at creation, even though
                    // the resource's private visibility also constrains its owner.
                    if !records.contains_key(&key("policy_versions", &identity.audience)) {
                        return Err(invalid(
                            "artifact must retain its registered audience policy",
                        ));
                    }
                    edge!("resources", &a.resource_id);
                    edge!("artifact_versions", version);
                    edge!("policy_versions", &identity.audience);
                }
                Record::ArtifactVersion(v) => {
                    let artifact = match required(&records, "artifacts", &v.artifact_id)? {
                        Record::Artifact(a) => a,
                        _ => return Err(invalid("artifact parent kind mismatch")),
                    };
                    if !matches!(required(&records,"resource_versions",&v.resource_version_id)?,Record::ResourceVersion(r) if r.resource_id==artifact.resource_id)
                    {
                        return Err(invalid("artifact content belongs to another resource"));
                    }
                    edge!("artifacts", &v.artifact_id);
                    edge!("resource_versions", &v.resource_version_id);
                    edge!("principals", &v.editor_id);
                }
                Record::Authority => {}
            }
            for next in edges {
                queue.push_back((next, depth + 1));
            }
        }
        if records.keys().any(|k| !visited.contains(k)) {
            return Err(invalid("bundle contains unreachable extra records"));
        }
        let mut versions = BTreeSet::new();
        let mut resources = BTreeSet::new();
        for record in records.values() {
            match record {
                Record::ResourceVersion(v)
                    if !versions.insert((v.resource_id.clone(), v.version_no.value())) =>
                {
                    return Err(invalid("duplicate resource version number"));
                }
                Record::Task(t) if !resources.insert(t.resource_id.clone()) => {
                    return Err(invalid("duplicate task resource identity"));
                }
                _ => {}
            }
        }
        Ok(Self {
            candidate,
            records,
            originals,
            identity,
            version_id,
        })
    }
    fn receipt(&self, id: &DomainId) -> Result<PublicationReceipt, StoreError> {
        Ok(PublicationReceipt {
            root_table: self.candidate.root().table.clone(),
            root_id: self.candidate.root().id.clone(),
            version_id: self.version_id.clone(),
            bundle_blob_id: id.clone(),
            sha256: Digest::new(hex(&self.candidate.sha256()))
                .map_err(|_| invalid("invalid bundle digest"))?,
        })
    }
}

impl LocalStore {
    pub async fn publish_bundle(
        &mut self,
        input: BundlePublication<'_>,
    ) -> Result<PublicationReceipt, StoreError> {
        let candidate = BundleCandidate::inspect(input.bytes, input.workspace)
            .map_err(|_| invalid("invalid canonical bundle envelope"))?;
        candidate
            .verify_digest(&input.sha256)
            .map_err(|_| invalid("bundle digest mismatch"))?;
        if input.record_blobs.len() > candidate.records().len()
            || input.payloads.len() > candidate.referenced_blobs().len()
        {
            return Err(invalid("publication allocation/payload count bound"));
        }
        let identity = self.verified_identity(input.workspace, input.actor).await?;
        let closure = ValidatedClosure::validate(candidate, identity)?;
        let receipt = closure.receipt(input.bundle_blob_id)?;
        let allocations = self.validate_allocations(&closure, &input).await?;
        let mut payloads = BTreeMap::new();
        let mut total = 0usize;
        for payload in input.payloads {
            total = total
                .checked_add(payload.bytes.len())
                .ok_or(invalid("payload byte bound"))?;
            if total > MAX_CONTENT_BYTES
                || payload.bytes.len() > MAX_BYTES
                || payloads.insert(payload.id, payload.bytes).is_some()
            {
                return Err(invalid("payload byte/count or duplicate identity bound"));
            }
        }
        let mut staged = Vec::new();
        let mut declared_total = 0usize;
        let mut seen = BTreeSet::new();
        for reference in closure.candidate.referenced_blobs() {
            if !seen.insert(&reference.id) {
                continue;
            }
            let size = usize::try_from(reference.byte_size.value())
                .map_err(|_| invalid("content size exceeds platform range"))?;
            declared_total = declared_total
                .checked_add(size)
                .ok_or(invalid("content byte bound"))?;
            if size > MAX_BYTES || declared_total > MAX_CONTENT_BYTES {
                return Err(invalid("foundation content byte bound"));
            }
            let blob = match required(&closure.records, "blobs", &reference.id)? {
                Record::Blob(b) => b,
                _ => return Err(invalid("wrong blob declaration kind")),
            };
            if reference.sha256 != blob.sha256 || reference.byte_size != blob.byte_size {
                return Err(invalid("declared blob metadata differs from typed record"));
            }
            let bytes = if let Some(bytes) = payloads.remove(&reference.id) {
                bytes.to_vec()
            } else {
                self.read_registered_blob(
                    input.workspace,
                    input.actor,
                    &reference.id,
                    blob.media_type.as_str(),
                )
                .await?
            };
            reference
                .verify(&bytes)
                .map_err(|_| invalid("raw content digest/size mismatch"))?;
            staged.push(StagedBlob {
                id: reference.id.clone(),
                path: relative(input.workspace, &reference.id),
                bytes,
                media: blob.media_type.as_str().into(),
            });
        }
        if !payloads.is_empty()
            || closure
                .records
                .values()
                .filter(|r| matches!(r, Record::Blob(_)))
                .count()
                != seen.len()
        {
            return Err(invalid("missing or unused raw content declaration"));
        }
        for (k, record) in &closure.records {
            if matches!(record, Record::Authority) {
                continue;
            }
            let original = closure
                .originals
                .get(k)
                .ok_or(invalid("missing original record"))?;
            let bytes = index_bytes(original, closure.candidate.root(), input.bundle_blob_id)?;
            staged.push(StagedBlob {
                id: allocations[k].clone(),
                path: object_path(input.workspace, k),
                bytes,
                media: INDEX_MEDIA.into(),
            });
        }
        staged.push(StagedBlob {
            id: input.bundle_blob_id.clone(),
            path: relative(input.workspace, input.bundle_blob_id),
            bytes: closure.candidate.canonical_bytes().to_vec(),
            media: BUNDLE_MEDIA.into(),
        });
        let mut all_ids = BTreeSet::new();
        for blob in &staged {
            if !all_ids.insert(&blob.id) || closure.identity.blob_ids().any(|id| id == &blob.id) {
                return Err(invalid("publication storage identities collide"));
            }
            self.verify_blob_metadata(input.workspace, input.actor, blob)
                .await?;
        }
        // All semantic validation precedes filesystem writes. Only unreferenced
        // immutable files may survive a failed database transaction.
        for blob in &staged {
            self.put_at(input.workspace, &blob.path, &blob.bytes)?;
        }
        let mut tx = self.connection.begin().await?;
        closure.identity.check_live(&mut tx).await?;
        for blob in &staged {
            insert_blob(
                &mut tx,
                input.workspace,
                input.actor,
                blob,
                created_at(&closure)?,
            )
            .await?;
        }
        publish_projection(
            &mut tx,
            &closure,
            input.workspace,
            input.actor,
            input.bundle_blob_id,
        )
        .await?;
        #[cfg(test)]
        commit_barrier(&self.status.database_path, "before_commit")?;
        tx.commit().await?;
        #[cfg(test)]
        commit_barrier(&self.status.database_path, "after_commit")?;
        Ok(receipt)
    }
    async fn validate_allocations(
        &mut self,
        closure: &ValidatedClosure,
        input: &BundlePublication<'_>,
    ) -> Result<BTreeMap<Key, DomainId>, StoreError> {
        let mut allocations = BTreeMap::new();
        for a in input.record_blobs {
            let k = key(&a.table, &a.id);
            if !closure.records.contains_key(&k)
                || matches!(closure.records[&k], Record::Authority)
                || allocations.insert(k, a.blob_id.clone()).is_some()
            {
                return Err(invalid("invalid or duplicate record storage allocation"));
            }
        }
        for (k, record) in &closure.records {
            if matches!(record, Record::Authority) {
                continue;
            }
            let path = object_path(input.workspace, k);
            let existing: Option<String> = sqlx::query_scalar(
                "SELECT id FROM local_blobs WHERE workspace_id=? AND storage_relpath=?",
            )
            .bind(input.workspace.as_str())
            .bind(&path)
            .fetch_optional(&mut self.connection)
            .await?;
            if let Some(existing) = existing {
                let id = DomainId::new(existing)
                    .map_err(|_| invalid("invalid retained record storage ID"))?;
                if allocations.get(k).is_some_and(|provided| provided != &id) {
                    return Err(invalid("retained record storage identity conflict"));
                }
                let bytes = self
                    .read_at(input.workspace, input.actor, &id, INDEX_MEDIA, &path)
                    .await?;
                let original = closure
                    .originals
                    .get(k)
                    .ok_or(invalid("missing original record"))?;
                if bytes != index_bytes(original, closure.candidate.root(), input.bundle_blob_id)? {
                    return Err(invalid("immutable record identity collision"));
                }
                allocations.insert(k.clone(), id);
            } else if !allocations.contains_key(k) {
                return Err(invalid("missing explicit record storage allocation"));
            }
        }
        Ok(allocations)
    }
    async fn verify_blob_metadata(
        &mut self,
        workspace: &DomainId,
        actor: &DomainId,
        blob: &StagedBlob,
    ) -> Result<(), StoreError> {
        let existing:Option<BlobMetadata>=sqlx::query_as("SELECT storage_relpath,sha256,byte_size,media_type,owner_actor_id,status,erased_at_us FROM local_blobs WHERE workspace_id=? AND id=?").bind(workspace.as_str()).bind(blob.id.as_str()).fetch_optional(&mut self.connection).await?;
        if existing.is_some_and(|r| {
            r != (
                blob.path.clone(),
                digest(&blob.bytes).to_vec(),
                blob.bytes.len() as i64,
                blob.media.clone(),
                actor.as_str().into(),
                "verified".into(),
                None,
            )
        }) {
            return Err(invalid("immutable published blob metadata conflict"));
        }
        Ok(())
    }
    pub async fn read_task_bundle(
        &mut self,
        workspace: &DomainId,
        actor: &DomainId,
        task: &DomainId,
    ) -> Result<PublishedBundle, StoreError> {
        let identity = self.verified_identity(workspace, actor).await?;
        let id:String=sqlx::query_scalar("SELECT r.canonical_blob_id FROM local_tasks t JOIN local_task_revisions r ON r.workspace_id=t.workspace_id AND r.id=t.current_revision_id AND r.task_id=t.id WHERE t.workspace_id=? AND t.id=? AND t.owner_actor_id=?").bind(workspace.as_str()).bind(task.as_str()).bind(actor.as_str()).fetch_optional(&mut self.connection).await?.ok_or(invalid("missing current task bundle"))?;
        self.read_published(
            workspace,
            actor,
            &DomainId::new(id).map_err(|_| invalid("invalid stored bundle ID"))?,
            identity,
            Some(task),
        )
        .await
    }
    pub async fn read_artifact_bundle(
        &mut self,
        workspace: &DomainId,
        actor: &DomainId,
        artifact: &DomainId,
    ) -> Result<PublishedBundle, StoreError> {
        let identity = self.verified_identity(workspace, actor).await?;
        let k = key("artifacts", artifact);
        let path = object_path(workspace, &k);
        let record_id: String = sqlx::query_scalar(
            "SELECT id FROM local_blobs WHERE workspace_id=? AND storage_relpath=?",
        )
        .bind(workspace.as_str())
        .bind(&path)
        .fetch_optional(&mut self.connection)
        .await?
        .ok_or(invalid("missing retained artifact"))?;
        let bytes = self
            .read_at(
                workspace,
                actor,
                &DomainId::new(record_id).map_err(|_| invalid("invalid artifact index ID"))?,
                INDEX_MEDIA,
                &path,
            )
            .await?;
        // The index stores the explicitly allocated publication blob ID alongside
        // the root wrapper (implemented below); it never derives an ID from a path.
        let index: RecordIndex =
            serde_json::from_slice(&bytes).map_err(|_| invalid("invalid artifact index"))?;
        if index
            .record
            .get("table")
            .and_then(serde_json::Value::as_str)
            != Some("artifacts")
            || index.record.get("id").and_then(serde_json::Value::as_str) != Some(artifact.as_str())
            || index.schema_version != 1
            || index.root.table != "artifacts"
            || &index.root.id != artifact
        {
            return Err(invalid("artifact index points to another root"));
        }
        let result = self
            .read_published(workspace, actor, &index.bundle_blob_id, identity, None)
            .await?;
        if result.receipt.root_table != "artifacts" || &result.receipt.root_id != artifact {
            return Err(invalid("artifact bundle points to another root"));
        }
        Ok(result)
    }
    async fn read_published(
        &mut self,
        workspace: &DomainId,
        actor: &DomainId,
        id: &DomainId,
        identity: VerifiedIdentity,
        task: Option<&DomainId>,
    ) -> Result<PublishedBundle, StoreError> {
        let bytes = self
            .read_registered_blob(workspace, actor, id, BUNDLE_MEDIA)
            .await?;
        let candidate = BundleCandidate::inspect(&bytes, workspace)
            .map_err(|_| invalid("invalid stored bundle"))?;
        if candidate.canonical_bytes() != bytes {
            return Err(invalid("stored bundle is not canonical"));
        }
        let closure = ValidatedClosure::validate(candidate, identity)?;
        if let Some(task) = task {
            if closure.candidate.purpose() != Purpose::Task || &closure.candidate.root().id != task
            {
                return Err(invalid("task projection points to another root"));
            }
        }
        verify_projection(&mut self.connection, &closure, workspace, actor, id).await?;
        // Reuses full semantic/blob/index checks without staging or mutating rows.
        self.verify_retained_closure(&closure, workspace, actor, id)
            .await?;
        Ok(PublishedBundle {
            receipt: closure.receipt(id)?,
            canonical_bytes: bytes,
        })
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordIndex {
    schema_version: i32,
    root: RecordKey,
    bundle_blob_id: DomainId,
    record: serde_json::Value,
}
fn index_bytes(
    c: &RecordCandidate,
    root: &RecordKey,
    bundle: &DomainId,
) -> Result<Vec<u8>, StoreError> {
    canonical(
        &serde_json::json!({"schema_version":1,"root":{"table":root.table,"id":root.id},"bundle_blob_id":bundle,"record":{"table":c.table,"id":c.id,"record_schema":c.record_schema,"record":c.record}}),
    )
}
fn created_at(c: &ValidatedClosure) -> Result<i64, StoreError> {
    c.records
        .get(&key(&c.candidate.root().table, &c.candidate.root().id))
        .and_then(Record::identity)
        .map(|(_, _, at)| at.unix_micros())
        .ok_or(invalid("missing typed root timestamp"))
}

async fn insert_blob(
    conn: &mut sqlx::SqliteConnection,
    workspace: &DomainId,
    actor: &DomainId,
    blob: &StagedBlob,
    created: i64,
) -> Result<(), StoreError> {
    let existing:Option<BlobMetadata>=sqlx::query_as("SELECT storage_relpath,sha256,byte_size,media_type,owner_actor_id,status,erased_at_us FROM local_blobs WHERE workspace_id=? AND id=?").bind(workspace.as_str()).bind(blob.id.as_str()).fetch_optional(&mut *conn).await?;
    let expected = (
        blob.path.clone(),
        digest(&blob.bytes).to_vec(),
        blob.bytes.len() as i64,
        blob.media.clone(),
        actor.as_str().to_owned(),
        "verified".into(),
        None,
    );
    if let Some(existing) = existing {
        if existing != expected {
            return Err(invalid("published blob changed during transaction"));
        }
        return Ok(());
    }
    sqlx::query("INSERT INTO local_blobs (id,workspace_id,created_at_us,owner_actor_id,storage_relpath,sha256,byte_size,media_type,status,erased_at_us) VALUES (?,?,?,?,?,?,?,?,'verified',NULL)").bind(blob.id.as_str()).bind(workspace.as_str()).bind(created).bind(actor.as_str()).bind(&blob.path).bind(digest(&blob.bytes).as_slice()).bind(blob.bytes.len() as i64).bind(&blob.media).execute(conn).await?;
    Ok(())
}
async fn publish_projection(
    conn: &mut sqlx::SqliteConnection,
    c: &ValidatedClosure,
    workspace: &DomainId,
    actor: &DomainId,
    bundle: &DomainId,
) -> Result<(), StoreError> {
    if c.candidate.purpose() == Purpose::Artifact {
        return Ok(());
    }
    let t = match required(&c.records, "tasks", &c.candidate.root().id)? {
        Record::Task(t) => t,
        _ => return Err(invalid("wrong task root")),
    };
    let r = match required(&c.records, "task_revisions", &c.version_id)? {
        Record::TaskRevision(r) => r,
        _ => return Err(invalid("wrong task revision root")),
    };
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM local_tasks WHERE workspace_id=? AND id=?)",
    )
    .bind(workspace.as_str())
    .bind(t.id.as_str())
    .fetch_one(&mut *conn)
    .await?;
    if exists {
        return verify_projection(conn, c, workspace, actor, bundle).await;
    }
    sqlx::query("INSERT INTO local_tasks (id,workspace_id,created_at_us,owner_actor_id,current_revision_id,authority,status,row_version) VALUES (?,?,?,?,NULL,'local',?,?)").bind(t.id.as_str()).bind(workspace.as_str()).bind(t.created_at.unix_micros()).bind(actor.as_str()).bind(&t.status).bind(t.row_version.value() as i64).execute(&mut *conn).await?;
    sqlx::query("INSERT INTO local_task_revisions (id,workspace_id,created_at_us,task_id,version_no,schema_version,canonical_blob_id,digest) VALUES (?,?,?,?,?,1,?,?)").bind(r.id.as_str()).bind(workspace.as_str()).bind(r.created_at.unix_micros()).bind(t.id.as_str()).bind(r.version_no).bind(bundle.as_str()).bind(c.candidate.sha256().as_slice()).execute(&mut *conn).await?;
    sqlx::query("UPDATE local_tasks SET current_revision_id=? WHERE workspace_id=? AND id=?")
        .bind(r.id.as_str())
        .bind(workspace.as_str())
        .bind(t.id.as_str())
        .execute(conn)
        .await?;
    Ok(())
}
async fn verify_projection(
    conn: &mut sqlx::SqliteConnection,
    c: &ValidatedClosure,
    workspace: &DomainId,
    actor: &DomainId,
    bundle: &DomainId,
) -> Result<(), StoreError> {
    if c.candidate.purpose() == Purpose::Artifact {
        return Ok(());
    }
    let t = match required(&c.records, "tasks", &c.candidate.root().id)? {
        Record::Task(t) => t,
        _ => return Err(invalid("wrong task root")),
    };
    let r = match required(&c.records, "task_revisions", &c.version_id)? {
        Record::TaskRevision(r) => r,
        _ => return Err(invalid("wrong task revision root")),
    };
    let row=sqlx::query("SELECT t.created_at_us,t.owner_actor_id,t.current_revision_id,t.authority,t.status,t.row_version,r.created_at_us AS revision_created,r.task_id,r.version_no,r.schema_version,r.canonical_blob_id,r.digest FROM local_tasks t JOIN local_task_revisions r ON r.workspace_id=t.workspace_id AND r.id=t.current_revision_id AND r.task_id=t.id WHERE t.workspace_id=? AND t.id=?").bind(workspace.as_str()).bind(t.id.as_str()).fetch_optional(conn).await?.ok_or(invalid("missing typed task projection"))?;
    if row.try_get::<i64, _>("created_at_us")? != t.created_at.unix_micros()
        || row.try_get::<String, _>("owner_actor_id")? != actor.as_str()
        || row.try_get::<String, _>("current_revision_id")? != r.id.as_str()
        || row.try_get::<String, _>("authority")? != "local"
        || row.try_get::<String, _>("status")? != t.status
        || row.try_get::<i64, _>("row_version")? as u64 != t.row_version.value()
        || row.try_get::<i64, _>("revision_created")? != r.created_at.unix_micros()
        || row.try_get::<String, _>("task_id")? != t.id.as_str()
        || row.try_get::<i32, _>("version_no")? != r.version_no
        || row.try_get::<i64, _>("schema_version")? != 1
        || row.try_get::<String, _>("canonical_blob_id")? != bundle.as_str()
        || row.try_get::<Vec<u8>, _>("digest")? != c.candidate.sha256()
    {
        return Err(invalid("task bundle and typed projections disagree"));
    }
    Ok(())
}
impl LocalStore {
    async fn verify_retained_closure(
        &mut self,
        c: &ValidatedClosure,
        workspace: &DomainId,
        actor: &DomainId,
        bundle: &DomainId,
    ) -> Result<(), StoreError> {
        for (k, record) in &c.records {
            if matches!(record, Record::Authority) {
                continue;
            }
            let path = object_path(workspace, k);
            let id: String = sqlx::query_scalar(
                "SELECT id FROM local_blobs WHERE workspace_id=? AND storage_relpath=?",
            )
            .bind(workspace.as_str())
            .bind(&path)
            .fetch_optional(&mut self.connection)
            .await?
            .ok_or(invalid("missing retained canonical record index"))?;
            let bytes = self
                .read_at(
                    workspace,
                    actor,
                    &DomainId::new(id).map_err(|_| invalid("invalid index blob ID"))?,
                    INDEX_MEDIA,
                    &path,
                )
                .await?;
            let original = c
                .originals
                .get(k)
                .ok_or(invalid("missing original record"))?;
            if bytes != index_bytes(original, c.candidate.root(), bundle)? {
                return Err(invalid("retained record index differs from bundle"));
            }
        }
        let mut seen = BTreeSet::new();
        let mut declared_total = 0usize;
        for reference in c.candidate.referenced_blobs() {
            if !seen.insert(&reference.id) {
                continue;
            }
            let size = usize::try_from(reference.byte_size.value())
                .map_err(|_| invalid("content size exceeds platform range"))?;
            declared_total = declared_total
                .checked_add(size)
                .ok_or(invalid("content byte bound"))?;
            if size > MAX_BYTES || declared_total > MAX_CONTENT_BYTES {
                return Err(invalid("foundation content byte bound"));
            }
            let blob = match required(&c.records, "blobs", &reference.id)? {
                Record::Blob(b) => b,
                _ => return Err(invalid("wrong payload type")),
            };
            if reference.sha256 != blob.sha256 || reference.byte_size != blob.byte_size {
                return Err(invalid("retained payload declarations disagree"));
            }
            let bytes = self
                .read_registered_blob(workspace, actor, &reference.id, blob.media_type.as_str())
                .await?;
            reference
                .verify(&bytes)
                .map_err(|_| invalid("retained payload digest or size differs"))?;
        }
        if c.records
            .values()
            .filter(|r| matches!(r, Record::Blob(_)))
            .count()
            != seen.len()
        {
            return Err(invalid("undeclared retained raw content"));
        }
        c.identity.check_live(&mut self.connection).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
fn commit_barrier(database: &std::path::Path, stage: &str) -> Result<(), StoreError> {
    if std::env::var("AVENCREW_PUBLICATION_CRASH_CHILD").as_deref() != Ok("1")
        || std::env::var("AVENCREW_PUBLICATION_CRASH_STAGE").as_deref() != Ok(stage)
    {
        return Ok(());
    }
    std::fs::write(
        database
            .parent()
            .ok_or(invalid("missing test root"))?
            .join("publication-crash-ready"),
        stage,
    )?;
    let started = std::time::Instant::now();
    while started.elapsed() < std::time::Duration::from_secs(30) {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    Err(invalid("publication crash test timed out"))
}
