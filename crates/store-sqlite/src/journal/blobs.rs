//! Minimal immutable canonical-content storage. Database references publish it.
//! Checkpoint coverage and general vault cleanup are owned by P2-06.
use super::{private_root, validate_file, LocalStore, StoreError};
use avencrew_contracts::{canonical_bytes, scalars::DomainId};
use sha2::{Digest as _, Sha256};
use sqlx::Row;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

pub(super) const MAX_BYTES: usize = 1_048_576;
pub(super) const REGISTRATION_MEDIA: &str = "application/vnd.avencrew.local-registration+json";
pub(super) const RECORD_MEDIA: &str = "application/vnd.avencrew.canonical-record+json";

pub(super) fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
pub(super) fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|b| {
            [
                HEX[(b >> 4) as usize] as char,
                HEX[(b & 15) as usize] as char,
            ]
        })
        .collect()
}
pub(super) fn canonical<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, StoreError> {
    let bytes = serde_json::to_vec(value).map_err(|_| invalid("invalid canonical record"))?;
    canonical_bytes(&bytes).map_err(|_| invalid("invalid canonical record"))
}
pub(super) fn invalid(reason: &'static str) -> StoreError {
    StoreError::Incompatible(reason)
}
pub(super) fn relative(workspace: &DomainId, id: &DomainId) -> String {
    format!("blobs/{}/{}.json", workspace.as_str(), id.as_str())
}
fn read_private(path: &Path) -> Result<Vec<u8>, StoreError> {
    validate_file(path)?;
    let file = OpenOptions::new().read(true).open(path)?;
    let opened = file.metadata()?;
    let named = fs::symlink_metadata(path)?;
    if opened.ino() != named.ino() || opened.dev() != named.dev() || opened.len() > MAX_BYTES as u64
    {
        return Err(invalid("canonical blob identity or size changed"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > MAX_BYTES {
        return Err(invalid("canonical blob exceeds byte bound"));
    }
    Ok(bytes)
}

impl LocalStore {
    pub(super) fn put_canonical(
        &self,
        workspace: &DomainId,
        id: &DomainId,
        bytes: &[u8],
    ) -> Result<String, StoreError> {
        if bytes.is_empty() || bytes.len() > MAX_BYTES {
            return Err(invalid("canonical blob byte bound"));
        }
        let root = self
            .status
            .database_path
            .parent()
            .ok_or(invalid("missing data root"))?;
        let blobs = private_root(&root.join("blobs"))?;
        let folder = private_root(&blobs.join(workspace.as_str()))?;
        File::open(root)?.sync_all()?;
        File::open(&blobs)?.sync_all()?;
        let path = folder.join(format!("{}.json", id.as_str()));
        let pending = folder.join(format!("{}.pending", id.as_str()));
        // One supervisor owns these paths. A crash may leave an unreferenced
        // staging file, or the two names from link-before-unlink. Recover only
        // this exact allocated ID; never follow a symlink or scan arbitrary paths.
        match fs::symlink_metadata(&pending) {
            Ok(m) => {
                if !m.is_file() || m.mode() & 0o777 != 0o600 || !(m.nlink() == 1 || m.nlink() == 2)
                {
                    return Err(invalid("unsafe canonical staging file"));
                }
                if m.nlink() == 2 {
                    let final_meta = fs::symlink_metadata(&path)?;
                    if m.ino() != final_meta.ino() || m.dev() != final_meta.dev() {
                        return Err(invalid("staging link does not match final content"));
                    }
                }
                fs::remove_file(&pending)?;
                File::open(&folder)?.sync_all()?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
        if fs::symlink_metadata(&path).is_ok() {
            if read_private(&path)? != bytes {
                return Err(invalid("immutable canonical blob conflict"));
            }
            File::open(&folder)?.sync_all()?;
            return Ok(relative(workspace, id));
        }
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&pending)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            // Atomic, no replacement of an existing immutable object.
            fs::hard_link(&pending, &path)?;
            fs::remove_file(&pending)?;
            File::open(&folder)?.sync_all()?;
            if read_private(&path)? != bytes {
                return Err(invalid("canonical publication bytes changed"));
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&pending);
        }
        result?;
        Ok(relative(workspace, id))
    }

    pub(super) async fn read_registered_blob(
        &mut self,
        workspace: &DomainId,
        actor: &DomainId,
        id: &DomainId,
        media: &str,
    ) -> Result<Vec<u8>, StoreError> {
        let row=sqlx::query("SELECT storage_relpath,sha256,byte_size,status,erased_at_us,owner_actor_id,media_type FROM local_blobs WHERE workspace_id=? AND id=?")
            .bind(workspace.as_str()).bind(id.as_str()).fetch_optional(&mut self.connection).await?
            .ok_or(invalid("missing registered canonical blob"))?;
        let path: String = row.try_get("storage_relpath")?;
        let hash: Vec<u8> = row.try_get("sha256")?;
        let size: i64 = row.try_get("byte_size")?;
        let state: String = row.try_get("status")?;
        let erased: Option<i64> = row.try_get("erased_at_us")?;
        let owner: String = row.try_get("owner_actor_id")?;
        let kind: String = row.try_get("media_type")?;
        if path != relative(workspace, id)
            || state != "verified"
            || erased.is_some()
            || owner != actor.as_str()
            || kind != media
            || !(0..=MAX_BYTES as i64).contains(&size)
        {
            return Err(invalid(
                "canonical blob is unavailable or has incompatible metadata",
            ));
        }
        let root = self
            .status
            .database_path
            .parent()
            .ok_or(invalid("missing data root"))?;
        // Validate each directory: a safe relative string alone does not reject
        // a symlinked or permissive ancestor.
        for directory in [
            root.to_path_buf(),
            root.join("blobs"),
            root.join("blobs").join(workspace.as_str()),
        ] {
            // Reads cannot create a missing registry directory.
            std::fs::symlink_metadata(&directory)?;
            private_root(&directory)?;
        }
        let bytes = read_private(&root.join(path))?;
        if bytes.len() != size as usize || hash != digest(&bytes) {
            return Err(invalid("registered canonical blob digest or size mismatch"));
        }
        Ok(bytes)
    }
}
