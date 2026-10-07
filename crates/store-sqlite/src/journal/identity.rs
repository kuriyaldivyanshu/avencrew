//! Standalone author provenance. Never server authentication or execution authority.
use super::blobs::{canonical, digest, hex, invalid, RECORD_MEDIA, REGISTRATION_MEDIA};
use super::{LocalStore, StoreError};
use avencrew_contracts::{
    canonical_bytes,
    scalars::{Counter, Digest, DomainId, Instant, Nullable, PositiveCounter, Text},
};
use serde::{Deserialize, Serialize};
use sqlx::{Connection, Row};
use std::collections::BTreeSet;

type Label = Text<0, 16384>;
type Nonempty = Text<1, 16384>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrationReceipt {
    pub registration_blob_id: DomainId,
    pub registration_sha256: Digest,
    pub workspace_id: DomainId,
    pub actor_id: DomainId,
    pub device_id: DomainId,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Registration {
    schema_version: String,
    id: DomainId,
    workspace_id: DomainId,
    actor_id: DomainId,
    device_id: DomainId,
    principal_id: DomainId,
    user_reference_id: DomainId,
    audience_policy_id: DomainId,
    execution_policy_id: DomainId,
    created_at: Instant,
    origin_public_key: Nonempty,
    permission_generation: Counter,
    deletion_generation: Counter,
    records: References,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Reference {
    blob_id: DomainId,
    record_id: DomainId,
    record_schema: String,
    sha256: Digest,
    byte_size: Counter,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct References {
    user_reference: Reference,
    principal: Reference,
    workspace: Reference,
    device: Reference,
    audience_policy: Reference,
    execution_policy: Reference,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct User {
    id: DomainId,
    created_at: Instant,
    row_version: PositiveCounter,
    updated_at: Instant,
    display_name: Label,
    status: String,
    erased_at: Nullable<Instant>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Principal {
    id: DomainId,
    workspace_id: DomainId,
    created_at: Instant,
    row_version: PositiveCounter,
    updated_at: Instant,
    kind: String,
    user_id: Nullable<DomainId>,
    display_name: Label,
    status: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Workspace {
    id: DomainId,
    created_at: Instant,
    row_version: PositiveCounter,
    updated_at: Instant,
    name: Label,
    origin_device_key: Nullable<Nonempty>,
    status: String,
    permission_generation: Counter,
    deletion_generation: Counter,
    settings: serde_json::Value,
    last_observation_seq: Counter,
    last_sync_seq: Counter,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Device {
    id: DomainId,
    workspace_id: DomainId,
    created_at: Instant,
    row_version: PositiveCounter,
    updated_at: Instant,
    principal_id: DomainId,
    public_key: Nonempty,
    label: Label,
    os: Label,
    architecture: Label,
    status: String,
    last_seen_at: Nullable<Instant>,
    minimum_deletion_generation: Counter,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Policy<R> {
    id: DomainId,
    workspace_id: DomainId,
    created_at: Instant,
    name: Label,
    version_no: i32,
    policy_kind: String,
    schema_version: i32,
    rules: R,
    digest: Digest,
    published_by_id: DomainId,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AudienceRules {
    allowed_principal_ids: Vec<DomainId>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ExecutionRules {
    default: String,
    allowed_tools: Vec<String>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Records {
    user_reference: User,
    principal: Principal,
    workspace: Workspace,
    device: Device,
    audience_policy: Policy<AudienceRules>,
    execution_policy: Policy<ExecutionRules>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bootstrap {
    registration: Registration,
    records: Records,
}

impl References {
    fn all(&self) -> [&Reference; 6] {
        [
            &self.user_reference,
            &self.principal,
            &self.workspace,
            &self.device,
            &self.audience_policy,
            &self.execution_policy,
        ]
    }
}
impl Records {
    fn bytes(&self) -> Result<[Vec<u8>; 6], StoreError> {
        Ok([
            canonical(&self.user_reference)?,
            canonical(&self.principal)?,
            canonical(&self.workspace)?,
            canonical(&self.device)?,
            canonical(&self.audience_policy)?,
            canonical(&self.execution_policy)?,
        ])
    }
}
impl Bootstrap {
    fn decode(bytes: &[u8]) -> Result<Self, StoreError> {
        let bytes = canonical_bytes(bytes)
            .map_err(|_| invalid("invalid or oversized standalone bootstrap JSON"))?;
        let request: Self = serde_json::from_slice(&bytes)
            .map_err(|_| invalid("invalid standalone bootstrap record shapes"))?;
        request.validate()?;
        Ok(request)
    }
    fn validate(&self) -> Result<(), StoreError> {
        let r = &self.registration;
        let v = &self.records;
        if r.schema_version != "avencrew.local-registration/1.0"
            || r.permission_generation.value() != 0
            || r.deletion_generation.value() != 0
        {
            return Err(invalid("unsupported standalone registration profile"));
        }
        let key = r.origin_public_key.as_str();
        if !key.len().is_multiple_of(2)
            || !key
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid("origin public key must be nonempty lowercase hex"));
        }
        let ids = [
            &r.user_reference_id,
            &r.principal_id,
            &r.workspace_id,
            &r.device_id,
            &r.audience_policy_id,
            &r.execution_policy_id,
        ];
        let record_ids = [
            &v.user_reference.id,
            &v.principal.id,
            &v.workspace.id,
            &v.device.id,
            &v.audience_policy.id,
            &v.execution_policy.id,
        ];
        if ids != record_ids
            || v.principal.workspace_id != r.workspace_id
            || v.device.workspace_id != r.workspace_id
            || v.audience_policy.workspace_id != r.workspace_id
            || v.execution_policy.workspace_id != r.workspace_id
            || v.device.principal_id != r.principal_id
            || v.principal.user_id.0.as_ref() != Some(&r.user_reference_id)
        {
            return Err(invalid("standalone identity mapping mismatch"));
        }
        if v.user_reference.status != "active"
            || v.user_reference.erased_at.0.is_some()
            || v.principal.kind != "human"
            || v.principal.status != "active"
            || v.workspace.status != "active"
            || v.device.status != "active"
            || v.workspace.origin_device_key.0.as_ref() != Some(&r.origin_public_key)
            || v.device.public_key != r.origin_public_key
        {
            return Err(invalid("standalone identity is inactive or incompatible"));
        }
        let times = [
            (
                &v.user_reference.created_at,
                &v.user_reference.updated_at,
                v.user_reference.row_version.value(),
            ),
            (
                &v.principal.created_at,
                &v.principal.updated_at,
                v.principal.row_version.value(),
            ),
            (
                &v.workspace.created_at,
                &v.workspace.updated_at,
                v.workspace.row_version.value(),
            ),
            (
                &v.device.created_at,
                &v.device.updated_at,
                v.device.row_version.value(),
            ),
        ];
        if times.into_iter().any(|(created, updated, version)| {
            created != &r.created_at || updated != created || version != 1
        }) || v.audience_policy.created_at != r.created_at
            || v.execution_policy.created_at != r.created_at
            || v.device.last_seen_at.0.is_some()
            || v.device.minimum_deletion_generation.value() != 0
            || v.workspace.permission_generation.value() != 0
            || v.workspace.deletion_generation.value() != 0
            || v.workspace.last_observation_seq.value() != 0
            || v.workspace.last_sync_seq.value() != 0
            || v.workspace.settings != serde_json::json!({})
        {
            return Err(invalid(
                "standalone bootstrap requires initial immutable provenance",
            ));
        }
        if v.audience_policy.policy_kind != "audience"
            || v.audience_policy.schema_version != 1
            || v.audience_policy.version_no != 1
            || v.audience_policy.published_by_id != r.principal_id
            || v.audience_policy.rules.allowed_principal_ids != [r.principal_id.clone()]
            || v.execution_policy.policy_kind != "execution"
            || v.execution_policy.schema_version != 1
            || v.execution_policy.version_no != 1
            || v.execution_policy.published_by_id != r.principal_id
            || v.execution_policy.rules.default != "deny"
            || !v.execution_policy.rules.allowed_tools.is_empty()
        {
            return Err(invalid(
                "standalone policy must restrict audience and deny dispatch",
            ));
        }
        if v.audience_policy.digest.as_str() != hex(&digest(&canonical(&v.audience_policy.rules)?))
            || v.execution_policy.digest.as_str()
                != hex(&digest(&canonical(&v.execution_policy.rules)?))
        {
            return Err(invalid("standalone policy rules digest mismatch"));
        }
        let bytes = v.bytes()?;
        let references = r.records.all();
        let mut blobs = BTreeSet::new();
        blobs.insert(&r.id);
        for ((reference, record_id), bytes) in references.into_iter().zip(ids).zip(&bytes) {
            if reference.record_schema != "server-v1:target"
                || &reference.record_id != record_id
                || !blobs.insert(&reference.blob_id)
                || reference.byte_size.value() != bytes.len() as u64
                || reference.sha256.as_str() != hex(&digest(bytes))
            {
                return Err(invalid("standalone record reference/digest mismatch"));
            }
        }
        Ok(())
    }
    fn receipt(&self) -> Result<RegistrationReceipt, StoreError> {
        let r = &self.registration;
        Ok(RegistrationReceipt {
            registration_blob_id: r.id.clone(),
            registration_sha256: Digest::new(hex(&digest(&canonical(r)?)))
                .map_err(|_| invalid("invalid registration digest"))?,
            workspace_id: r.workspace_id.clone(),
            actor_id: r.actor_id.clone(),
            device_id: r.device_id.clone(),
        })
    }
}

impl LocalStore {
    /// Trusted supervisor bootstrap only; no model/client RPC exposes this API.
    /// Retains offline author references, not server accounts or execution grants.
    pub async fn bootstrap_standalone(
        &mut self,
        bytes: &[u8],
        keychain_locator: &str,
    ) -> Result<RegistrationReceipt, StoreError> {
        let request = Bootstrap::decode(bytes)?;
        let receipt = request.receipt()?;
        if keychain_locator.is_empty()
            || keychain_locator.chars().count() > 16384
            || keychain_locator.contains('\0')
        {
            return Err(invalid("invalid OS keychain locator"));
        }
        let r = &request.registration;
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM local_workspaces WHERE id=?)")
                .bind(r.workspace_id.as_str())
                .fetch_one(&mut self.connection)
                .await?;
        if exists {
            let retained = self.resolve_identity(&r.workspace_id, &r.actor_id).await?;
            if retained.receipt()? != receipt {
                return Err(invalid("standalone registration conflict"));
            }
            let locator: String = sqlx::query_scalar(
                "SELECT keychain_locator FROM local_devices WHERE workspace_id=? AND id=?",
            )
            .bind(r.workspace_id.as_str())
            .bind(r.device_id.as_str())
            .fetch_one(&mut self.connection)
            .await?;
            if locator != keychain_locator {
                return Err(invalid("standalone keychain locator conflict"));
            }
            return Ok(receipt);
        }
        let records = request.records.bytes()?;
        let registration = canonical(r)?;
        let refs = r.records.all();
        let mut paths = Vec::with_capacity(7);
        for (reference, bytes) in refs.iter().zip(&records) {
            paths.push(self.put_canonical(&r.workspace_id, &reference.blob_id, bytes)?);
        }
        paths.push(self.put_canonical(&r.workspace_id, &r.id, &registration)?);
        let mut transaction = self.connection.begin().await?;
        let created = r.created_at.unix_micros();
        sqlx::query("INSERT INTO local_workspaces (id,workspace_id,created_at_us,name,origin_public_key,status,permission_generation,deletion_generation,last_local_seq) VALUES (?,?,?,?,?,'active',0,0,0)")
            .bind(r.workspace_id.as_str())
            .bind(r.workspace_id.as_str())
            .bind(created)
            .bind(request.records.workspace.name.as_str())
            .bind(r.origin_public_key.as_str())
            .execute(&mut *transaction)
            .await?;
        sqlx::query("INSERT INTO local_actors (id,workspace_id,created_at_us,kind,display_name,status,authority_expires_at_us) VALUES (?,?,?,'human',?,'active',NULL)")
            .bind(r.actor_id.as_str())
            .bind(r.workspace_id.as_str())
            .bind(created)
            .bind(request.records.principal.display_name.as_str())
            .execute(&mut *transaction)
            .await?;
        sqlx::query("INSERT INTO local_devices (id,workspace_id,created_at_us,actor_id,public_key,keychain_locator,status) VALUES (?,?,?,?,?,?,'active')")
            .bind(r.device_id.as_str())
            .bind(r.workspace_id.as_str())
            .bind(created)
            .bind(r.actor_id.as_str())
            .bind(r.origin_public_key.as_str())
            .bind(keychain_locator)
            .execute(&mut *transaction)
            .await?;
        for (index, bytes) in records
            .iter()
            .chain(std::iter::once(&registration))
            .enumerate()
        {
            let id = if index == 6 {
                &r.id
            } else {
                &refs[index].blob_id
            };
            let media = if index == 6 {
                REGISTRATION_MEDIA
            } else {
                RECORD_MEDIA
            };
            sqlx::query("INSERT INTO local_blobs (id,workspace_id,created_at_us,owner_actor_id,storage_relpath,sha256,byte_size,media_type,status,erased_at_us) VALUES (?,?,?,?,?,?,?,?,'verified',NULL)")
                .bind(id.as_str())
                .bind(r.workspace_id.as_str())
                .bind(created)
                .bind(r.actor_id.as_str())
                .bind(&paths[index])
                .bind(digest(bytes).as_slice())
                .bind(bytes.len() as i64)
                .bind(media)
                .execute(&mut *transaction)
                .await?;
        }
        #[cfg(test)]
        commit_barrier(&self.status.database_path, "before_commit")?;
        transaction.commit().await?;
        #[cfg(test)]
        commit_barrier(&self.status.database_path, "after_commit")?;
        Ok(receipt)
    }

    /// Revalidates retained bytes, projections, availability and generations.
    /// The receipt is local provenance; it confers no logged-in server authority.
    pub async fn resolve_standalone(
        &mut self,
        workspace: &DomainId,
        actor: &DomainId,
    ) -> Result<RegistrationReceipt, StoreError> {
        self.resolve_identity(workspace, actor).await?.receipt()
    }

    async fn resolve_identity(
        &mut self,
        workspace: &DomainId,
        actor: &DomainId,
    ) -> Result<Bootstrap, StoreError> {
        // Suppression/revocation must block payload reads, not merely reject a
        // receipt after protected bytes have already been loaded into memory.
        let now = i64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| invalid("clock before epoch"))?
                .as_micros(),
        )
        .map_err(|_| invalid("clock exceeds journal range"))?;
        let available: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM local_workspaces w JOIN local_actors a ON a.workspace_id=w.id WHERE w.id=? AND a.id=? AND w.status='active' AND w.permission_generation=0 AND w.deletion_generation=0 AND a.kind='human' AND a.status='active' AND (a.authority_expires_at_us IS NULL OR a.authority_expires_at_us>?) AND NOT EXISTS(SELECT 1 FROM local_erasure_fences f WHERE f.workspace_id=w.id))")
            .bind(workspace.as_str()).bind(actor.as_str()).bind(now).fetch_one(&mut self.connection).await?;
        if !available {
            return Err(invalid(
                "standalone identity unavailable before content lookup",
            ));
        }
        let ids:Vec<String>=sqlx::query_scalar("SELECT id FROM local_blobs WHERE workspace_id=? AND owner_actor_id=? AND media_type=? LIMIT 2").bind(workspace.as_str()).bind(actor.as_str()).bind(REGISTRATION_MEDIA).fetch_all(&mut self.connection).await?;
        if ids.len() != 1 {
            return Err(invalid("missing or ambiguous retained registration"));
        }
        let id = DomainId::new(&ids[0]).map_err(|_| invalid("invalid retained registration ID"))?;
        let bytes = self
            .read_registered_blob(workspace, actor, &id, REGISTRATION_MEDIA)
            .await?;
        if canonical_bytes(&bytes).map_err(|_| invalid("invalid retained registration JSON"))?
            != bytes
        {
            return Err(invalid("retained registration is not canonical"));
        }
        let registration: Registration = serde_json::from_slice(&bytes)
            .map_err(|_| invalid("invalid retained registration shape"))?;
        if registration.id != id
            || registration.workspace_id != *workspace
            || registration.actor_id != *actor
            || registration.schema_version != "avencrew.local-registration/1.0"
            || registration.permission_generation.value() != 0
            || registration.deletion_generation.value() != 0
        {
            return Err(invalid("retained registration identity mismatch"));
        }
        let device_available: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM local_devices WHERE workspace_id=? AND id=? AND actor_id=? AND public_key=? AND status='active')")
            .bind(workspace.as_str()).bind(registration.device_id.as_str()).bind(actor.as_str()).bind(registration.origin_public_key.as_str()).fetch_one(&mut self.connection).await?;
        if !device_available {
            return Err(invalid(
                "standalone device unavailable before record lookup",
            ));
        }
        let mut values = Vec::with_capacity(6);
        let mut total_bytes = bytes.len();
        for reference in registration.records.all() {
            let bytes = self
                .read_registered_blob(workspace, actor, &reference.blob_id, RECORD_MEDIA)
                .await?;
            total_bytes += bytes.len();
            if total_bytes > super::blobs::MAX_BYTES {
                return Err(invalid(
                    "retained registration exceeds aggregate byte bound",
                ));
            }
            if reference.sha256.as_str() != hex(&digest(&bytes))
                || reference.byte_size.value() != bytes.len() as u64
                || canonical_bytes(&bytes).map_err(|_| invalid("invalid retained record JSON"))?
                    != bytes
            {
                return Err(invalid("retained record reference mismatch"));
            }
            values.push(
                serde_json::from_slice::<serde_json::Value>(&bytes)
                    .map_err(|_| invalid("invalid retained record JSON"))?,
            );
        }
        let records:Records=serde_json::from_value(serde_json::json!({"user_reference":values[0],"principal":values[1],"workspace":values[2],"device":values[3],"audience_policy":values[4],"execution_policy":values[5]})).map_err(|_|invalid("invalid retained record shape"))?;
        let request = Bootstrap {
            registration,
            records,
        };
        request.validate()?;
        self.verify_identity_projection(&request).await?;
        Ok(request)
    }

    async fn verify_identity_projection(&mut self, request: &Bootstrap) -> Result<(), StoreError> {
        let r = &request.registration;
        let workspace=sqlx::query("SELECT created_at_us,name,origin_public_key,status,permission_generation,deletion_generation FROM local_workspaces WHERE id=?").bind(r.workspace_id.as_str()).fetch_one(&mut self.connection).await?;
        let actor=sqlx::query("SELECT created_at_us,kind,display_name,status,authority_expires_at_us FROM local_actors WHERE workspace_id=? AND id=?").bind(r.workspace_id.as_str()).bind(r.actor_id.as_str()).fetch_one(&mut self.connection).await?;
        let device=sqlx::query("SELECT created_at_us,actor_id,public_key,status FROM local_devices WHERE workspace_id=? AND id=?").bind(r.workspace_id.as_str()).bind(r.device_id.as_str()).fetch_one(&mut self.connection).await?;
        let fenced: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM local_erasure_fences WHERE workspace_id=?)",
        )
        .bind(r.workspace_id.as_str())
        .fetch_one(&mut self.connection)
        .await?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| invalid("clock before epoch"))?
            .as_micros();
        let expiry: Option<i64> = actor.try_get("authority_expires_at_us")?;
        if workspace.try_get::<i64, _>("created_at_us")? != r.created_at.unix_micros()
            || workspace.try_get::<String, _>("name")? != request.records.workspace.name.as_str()
            || workspace.try_get::<String, _>("origin_public_key")? != r.origin_public_key.as_str()
            || workspace.try_get::<String, _>("status")? != "active"
            || workspace.try_get::<i64, _>("permission_generation")? as u64
                != r.permission_generation.value()
            || workspace.try_get::<i64, _>("deletion_generation")? as u64
                != r.deletion_generation.value()
            || actor.try_get::<i64, _>("created_at_us")? != r.created_at.unix_micros()
            || actor.try_get::<String, _>("kind")? != "human"
            || actor.try_get::<String, _>("display_name")?
                != request.records.principal.display_name.as_str()
            || actor.try_get::<String, _>("status")? != "active"
            || expiry.is_some_and(|t| t < 0 || t as u128 <= now)
            || device.try_get::<i64, _>("created_at_us")? != r.created_at.unix_micros()
            || device.try_get::<String, _>("actor_id")? != r.actor_id.as_str()
            || device.try_get::<String, _>("public_key")? != r.origin_public_key.as_str()
            || device.try_get::<String, _>("status")? != "active"
            || fenced
        {
            return Err(invalid(
                "retained identity projection is unavailable or stale",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
pub(super) mod tests;

// Compiled only into unit tests: interrupt the actual registration transaction,
// never an approximation in a scratch database. No runtime environment hook.
#[cfg(test)]
fn commit_barrier(database: &std::path::Path, stage: &str) -> Result<(), StoreError> {
    if std::env::var("AVENCREW_IDENTITY_CRASH_CHILD").as_deref() != Ok("1")
        || std::env::var("AVENCREW_IDENTITY_CRASH_STAGE").as_deref() != Ok(stage)
    {
        return Ok(());
    }
    let root = database.parent().ok_or(invalid("missing test root"))?;
    std::fs::write(root.join("identity-crash-ready"), stage)?;
    let started = std::time::Instant::now();
    while started.elapsed() < std::time::Duration::from_secs(30) {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    Err(invalid("registration crash test timed out"))
}

// Private to the store. Neither callers nor bundle records can construct trust.
pub(super) struct VerifiedIdentity {
    pub principal: DomainId,
    pub audience: DomainId,
    pub execution: DomainId,
    pub records: std::collections::BTreeMap<(String, DomainId), serde_json::Value>,
    workspace: DomainId,
    actor: DomainId,
    device: DomainId,
    origin: String,
    retained: Vec<(DomainId, String, i64)>,
}
impl LocalStore {
    pub(super) async fn verified_identity(
        &mut self,
        workspace: &DomainId,
        actor: &DomainId,
    ) -> Result<VerifiedIdentity, StoreError> {
        let request = self.resolve_identity(workspace, actor).await?;
        let r = &request.registration;
        let mut records = std::collections::BTreeMap::new();
        records.insert(
            ("principals".into(), r.principal_id.clone()),
            serde_json::to_value(&request.records.principal)
                .map_err(|_| invalid("invalid retained principal"))?,
        );
        records.insert(
            ("policy_versions".into(), r.audience_policy_id.clone()),
            serde_json::to_value(&request.records.audience_policy)
                .map_err(|_| invalid("invalid retained policy"))?,
        );
        records.insert(
            ("policy_versions".into(), r.execution_policy_id.clone()),
            serde_json::to_value(&request.records.execution_policy)
                .map_err(|_| invalid("invalid retained policy"))?,
        );
        let mut retained: Vec<_> = r
            .records
            .all()
            .into_iter()
            .map(|x| {
                (
                    x.blob_id.clone(),
                    x.sha256.as_str().to_owned(),
                    x.byte_size.value() as i64,
                )
            })
            .collect();
        let bytes = canonical(r)?;
        retained.push((r.id.clone(), hex(&digest(&bytes)), bytes.len() as i64));
        Ok(VerifiedIdentity {
            principal: r.principal_id.clone(),
            audience: r.audience_policy_id.clone(),
            execution: r.execution_policy_id.clone(),
            records,
            workspace: workspace.clone(),
            actor: actor.clone(),
            device: r.device_id.clone(),
            origin: r.origin_public_key.as_str().into(),
            retained,
        })
    }
}
impl VerifiedIdentity {
    pub fn blob_ids(&self) -> impl Iterator<Item = &DomainId> {
        self.retained.iter().map(|(id, _, _)| id)
    }
    pub async fn check_live(&self, conn: &mut sqlx::SqliteConnection) -> Result<(), StoreError> {
        let now = i64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| invalid("clock before epoch"))?
                .as_micros(),
        )
        .map_err(|_| invalid("clock exceeds journal range"))?;
        let available:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM local_workspaces w JOIN local_actors a ON a.workspace_id=w.id JOIN local_devices d ON d.workspace_id=w.id AND d.actor_id=a.id WHERE w.id=? AND a.id=? AND d.id=? AND w.status='active' AND w.permission_generation=0 AND w.deletion_generation=0 AND w.origin_public_key=? AND a.kind='human' AND a.status='active' AND (a.authority_expires_at_us IS NULL OR a.authority_expires_at_us>?) AND d.status='active' AND d.public_key=? AND NOT EXISTS(SELECT 1 FROM local_erasure_fences f WHERE f.workspace_id=w.id))")
            .bind(self.workspace.as_str()).bind(self.actor.as_str()).bind(self.device.as_str()).bind(&self.origin).bind(now).bind(&self.origin).fetch_one(&mut *conn).await?;
        if !available {
            return Err(invalid("bundle publication identity is unavailable"));
        }
        for (id, hash, size) in &self.retained {
            let available:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM local_blobs WHERE workspace_id=? AND id=? AND owner_actor_id=? AND status='verified' AND erased_at_us IS NULL AND lower(hex(sha256))=? AND byte_size=?)").bind(self.workspace.as_str()).bind(id.as_str()).bind(self.actor.as_str()).bind(hash).bind(size).fetch_one(&mut *conn).await?;
            if !available {
                return Err(invalid("bundle publication registration changed"));
            }
        }
        Ok(())
    }
}
