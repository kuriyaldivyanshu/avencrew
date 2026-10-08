//! Opaque immutable bytes. Vault possession is not authority to execute or publish.
use super::blobs::{hex, invalid};
use super::{private_root, LocalStore, StoreError};
use avencrew_contracts::scalars::{Counter, Digest, DomainId, Instant};
use sha2::{Digest as _, Sha256};
use sqlx::{Connection, Row};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const MAX_BYTES: u64 = 64 * 1024 * 1024;
const MEDIA: &str = "application/vnd.avencrew.vault-bytes";
const RETENTION: Duration = Duration::from_secs(24 * 60 * 60);

pub struct BlobImport<'a> {
    pub workspace: &'a DomainId,
    pub actor: &'a DomainId,
    pub id: &'a DomainId,
    pub sha256: &'a Digest,
    pub byte_size: &'a Counter,
    pub at: &'a Instant,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultReceipt {
    pub id: DomainId,
    pub sha256: Digest,
    pub byte_size: Counter,
}
#[derive(Debug, PartialEq, Eq)]
pub struct StagingCleanup {
    pub inspected: usize,
    pub removed: usize,
}
fn locator(w: &DomainId, id: &DomainId) -> String {
    format!("blobs/{}/vault/{}.bin", w.as_str(), id.as_str())
}
fn file(path: &Path, uid: u32) -> Result<File, StoreError> {
    let named = fs::symlink_metadata(path)?;
    if !named.is_file() || named.nlink() != 1 || named.mode() & 0o777 != 0o600 || named.uid() != uid
    {
        return Err(invalid("unsafe vault file"));
    }
    let f = File::open(path)?;
    let opened = f.metadata()?;
    if opened.ino() != named.ino() || opened.dev() != named.dev() || opened.len() > MAX_BYTES {
        return Err(invalid("vault file identity or size changed"));
    }
    Ok(f)
}
fn verify(mut f: File, hash: &Digest, size: u64) -> Result<File, StoreError> {
    if size > MAX_BYTES || f.metadata()?.len() != size {
        return Err(invalid("vault size mismatch"));
    }
    let before = f.metadata()?;
    let mut h = Sha256::new();
    let mut count = 0u64;
    let mut buf = [0; 32768];
    loop {
        let n = match f.read(&mut buf) {
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            other => other?,
        };
        if n == 0 {
            break;
        }
        count += n as u64;
        if count > size {
            return Err(invalid("vault bytes exceed declared size"));
        }
        h.update(&buf[..n]);
    }
    let after = f.metadata()?;
    if count != size
        || hex(&h.finalize()) != hash.as_str()
        || before.len() != after.len()
        || before.mtime() != after.mtime()
        || before.mtime_nsec() != after.mtime_nsec()
    {
        return Err(invalid("vault integrity mismatch"));
    }
    f.seek(SeekFrom::Start(0))?;
    Ok(f)
}
impl LocalStore {
    fn vault_folder(&self, w: &DomainId, create: bool) -> Result<(PathBuf, u32), StoreError> {
        let root = self
            .status
            .database_path
            .parent()
            .ok_or(invalid("missing vault root"))?;
        let uid = fs::symlink_metadata(root)?.uid();
        let mut path = root.to_path_buf();
        for part in ["blobs", w.as_str(), "vault"] {
            path.push(part);
            if !create {
                fs::symlink_metadata(&path)?;
            }
            private_root(&path)?;
            if fs::symlink_metadata(&path)?.uid() != uid {
                return Err(invalid("vault directory owner mismatch"));
            }
            if create {
                File::open(path.parent().ok_or(invalid("missing vault parent"))?)?.sync_all()?;
            }
        }
        Ok((path, uid))
    }
    fn vault_staging_folder(&self, w: &DomainId, create: bool) -> Result<PathBuf, StoreError> {
        let (folder, uid) = self.vault_folder(w, create)?;
        let staging = folder.join("staging");
        if !create {
            fs::symlink_metadata(&staging)?;
        }
        private_root(&staging)?;
        if fs::symlink_metadata(&staging)?.uid() != uid {
            return Err(invalid("vault staging owner mismatch"));
        }
        if create {
            File::open(folder)?.sync_all()?;
        }
        Ok(staging)
    }
    pub async fn import_vault_blob(
        &mut self,
        input: BlobImport<'_>,
        mut source: impl Read,
    ) -> Result<VaultReceipt, StoreError> {
        let size = input.byte_size.value();
        if size > MAX_BYTES {
            return Err(invalid("vault object limit exceeded"));
        }
        let identity = self.verified_identity(input.workspace, input.actor).await?;
        let existing = sqlx::query("SELECT * FROM local_blobs WHERE workspace_id=? AND id=?")
            .bind(input.workspace.as_str())
            .bind(input.id.as_str())
            .fetch_optional(&mut self.connection)
            .await?;
        if let Some(row) = existing.as_ref() {
            if row.try_get::<String, _>("storage_relpath")? != locator(input.workspace, input.id)
                || row.try_get::<String, _>("owner_actor_id")? != input.actor.as_str()
                || row.try_get::<String, _>("media_type")? != MEDIA
                || row.try_get::<String, _>("status")? != "verified"
                || row.try_get::<Option<i64>, _>("erased_at_us")?.is_some()
                || row.try_get::<i64, _>("byte_size")? != size as i64
                || hex(&row.try_get::<Vec<u8>, _>("sha256")?) != input.sha256.as_str()
            {
                return Err(invalid("vault ID metadata conflict"));
            }
        }
        let (folder, uid) = self.vault_folder(input.workspace, true)?;
        let final_path = folder.join(format!("{}.bin", input.id.as_str()));
        let staging = self.vault_staging_folder(input.workspace, true)?;
        let pending = staging.join(format!("{}.pending", input.id.as_str()));
        match fs::symlink_metadata(&pending) {
            Ok(m) => {
                let referenced:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM local_blobs WHERE workspace_id=? AND storage_relpath=?)")
                    .bind(input.workspace.as_str()).bind(format!("blobs/{}/vault/staging/{}.pending",input.workspace.as_str(),input.id.as_str())).fetch_one(&mut self.connection).await?;
                if referenced {
                    return Err(invalid("registered vault staging file cannot be replaced"));
                }
                if !m.is_file()
                    || m.mode() & 0o777 != 0o600
                    || m.uid() != uid
                    || !(m.nlink() == 1 || m.nlink() == 2)
                {
                    return Err(invalid("unsafe vault staging file"));
                }
                if m.nlink() == 2 {
                    let other = fs::symlink_metadata(&final_path)?;
                    if m.ino() != other.ino() || m.dev() != other.dev() {
                        return Err(invalid("unrecognized vault staging hard link"));
                    }
                }
                fs::remove_file(&pending)?;
                File::open(&staging)?.sync_all()?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
        let mut created_link = None;
        let result = (|| {
            let mut out = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&pending)?;
            let mut h = Sha256::new();
            let mut count = 0u64;
            let mut buf = [0; 32768];
            loop {
                let n = match source.read(&mut buf) {
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    other => other?,
                };
                if n == 0 {
                    break;
                }
                count += n as u64;
                if count > size {
                    return Err(invalid("vault import exceeds declared size"));
                }
                h.update(&buf[..n]);
                out.write_all(&buf[..n])?;
            }
            if count != size || hex(&h.finalize()) != input.sha256.as_str() {
                return Err(invalid("vault import digest or size mismatch"));
            }
            out.sync_all()?;
            let written = out.metadata()?;
            drop(out);
            match fs::symlink_metadata(&final_path) {
                Ok(_) => {
                    verify(file(&final_path, uid)?, input.sha256, size)?;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    fs::hard_link(&pending, &final_path)?;
                    created_link = Some((written.dev(), written.ino()));
                }
                Err(e) => return Err(e.into()),
            }
            fs::remove_file(&pending)?;
            File::open(&staging)?.sync_all()?;
            File::open(&folder)?.sync_all()?;
            verify(file(&final_path, uid)?, input.sha256, size)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&pending);
        }
        let published: Result<VaultReceipt, StoreError> = async {
        result?;
        let mut tx = self.connection.begin().await?;
        identity.check_live(&mut tx).await?;
        // This store owns the only writer; metadata was checked before any copy.
        if existing.is_none() {
            let digest: Vec<u8> = (0..64)
                .step_by(2)
                .map(|i| {
                    u8::from_str_radix(&input.sha256.as_str()[i..i + 2], 16)
                        .expect("validated digest")
                })
                .collect();
            sqlx::query("INSERT INTO local_blobs (id,workspace_id,created_at_us,owner_actor_id,storage_relpath,sha256,byte_size,media_type,status,erased_at_us) VALUES (?,?,?,?,?,?,?,?,'verified',NULL)")
                .bind(input.id.as_str()).bind(input.workspace.as_str()).bind(input.at.unix_micros()).bind(input.actor.as_str()).bind(locator(input.workspace,input.id)).bind(digest).bind(size as i64).bind(MEDIA).execute(&mut *tx).await?;
        }
        #[cfg(test)]
        tests::commit_barrier(&self.status.database_path, "before_commit")?;
        tx.commit().await?;
        #[cfg(test)]
        tests::commit_barrier(&self.status.database_path, "after_commit")?;
        Ok(VaultReceipt {
            id: input.id.clone(),
            sha256: input.sha256.clone(),
            byte_size: input.byte_size.clone(),
        })
        }.await;
        if published.is_err() {
            if let Some((dev, ino)) = created_link {
                // Commit errors can be uncertain. Remove only our own final
                // link after a successful authoritative absence query.
                let referenced = sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS(SELECT 1 FROM local_blobs WHERE storage_relpath=?)",
                )
                .bind(locator(input.workspace, input.id))
                .fetch_one(&mut self.connection)
                .await;
                if matches!(referenced, Ok(false)) {
                    if let Ok(m) = fs::symlink_metadata(&final_path) {
                        if m.is_file()
                            && m.dev() == dev
                            && m.ino() == ino
                            && m.nlink() == 1
                            && m.uid() == uid
                            && m.mode() & 0o777 == 0o600
                        {
                            fs::remove_file(&final_path)?;
                            File::open(&folder)?.sync_all()?;
                        }
                    }
                }
            }
        }
        published
    }

    pub async fn open_vault_blob(
        &mut self,
        w: &DomainId,
        a: &DomainId,
        id: &DomainId,
    ) -> Result<File, StoreError> {
        let identity = self.verified_identity(w, a).await?;
        let row = sqlx::query("SELECT * FROM local_blobs WHERE workspace_id=? AND id=?")
            .bind(w.as_str())
            .bind(id.as_str())
            .fetch_optional(&mut self.connection)
            .await?
            .ok_or(invalid("missing vault metadata"))?;
        if row.try_get::<String, _>("storage_relpath")? != locator(w, id)
            || row.try_get::<String, _>("owner_actor_id")? != a.as_str()
            || row.try_get::<String, _>("media_type")? != MEDIA
            || row.try_get::<String, _>("status")? != "verified"
            || row.try_get::<Option<i64>, _>("erased_at_us")?.is_some()
        {
            return Err(invalid("vault unavailable to caller"));
        }
        let size = row.try_get::<i64, _>("byte_size")?;
        let hash = Digest::new(hex(&row.try_get::<Vec<u8>, _>("sha256")?))
            .map_err(|_| invalid("invalid vault digest"))?;
        let (folder, uid) = self.vault_folder(w, false)?;
        let f = verify(
            file(&folder.join(format!("{}.bin", id.as_str())), uid)?,
            &hash,
            u64::try_from(size).map_err(|_| invalid("invalid vault size"))?,
        )?;
        identity.check_live(&mut self.connection).await?;
        Ok(f)
    }
    pub async fn cleanup_vault_staging(
        &mut self,
        w: &DomainId,
        a: &DomainId,
        limit: usize,
    ) -> Result<StagingCleanup, StoreError> {
        if !(1..=64).contains(&limit) {
            return Err(invalid("vault cleanup scan limit"));
        }
        let identity = self.verified_identity(w, a).await?;
        let root = self
            .status
            .database_path
            .parent()
            .ok_or(invalid("missing vault root"))?;
        let path = root.join(format!("blobs/{}/vault/staging", w.as_str()));
        if !path.try_exists()? {
            return Ok(StagingCleanup {
                inspected: 0,
                removed: 0,
            });
        }
        let (folder, uid) = self.vault_folder(w, false)?;
        let staging = self.vault_staging_folder(w, false)?;
        let now = SystemTime::now();
        let mut result = StagingCleanup {
            inspected: 0,
            removed: 0,
        };
        for entry in fs::read_dir(&staging)?.take(limit) {
            let entry = entry?;
            result.inspected += 1;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some(stem) = name.strip_suffix(".pending") else {
                continue;
            };
            let Ok(id) = DomainId::new(stem) else {
                continue;
            };
            let m = fs::symlink_metadata(entry.path())?;
            if !m.is_file()
                || m.mode() & 0o777 != 0o600
                || m.uid() != uid
                || !(m.nlink() == 1 || m.nlink() == 2)
                || now.duration_since(m.modified()?).unwrap_or_default() < RETENTION
            {
                continue;
            }
            if m.nlink() == 2 {
                let other = match fs::symlink_metadata(folder.join(format!("{stem}.bin"))) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                if m.ino() != other.ino() || m.dev() != other.dev() {
                    continue;
                }
            }
            let referenced:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM local_blobs WHERE workspace_id=? AND storage_relpath=?)").bind(w.as_str()).bind(format!("blobs/{}/vault/staging/{}.pending",w.as_str(),id.as_str())).fetch_one(&mut self.connection).await?;
            if referenced {
                continue;
            }
            identity.check_live(&mut self.connection).await?;
            fs::remove_file(entry.path())?;
            result.removed += 1;
        }
        if result.removed > 0 {
            File::open(&staging)?.sync_all()?;
        }
        Ok(result)
    }
}
#[cfg(test)]
mod tests;
