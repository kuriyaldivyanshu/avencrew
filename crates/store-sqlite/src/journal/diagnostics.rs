//! Fixed operational allowlist. Never serialize journal rows or subtract secrets.
use super::{LocalStore, StoreError};
use avencrew_contracts::scalars::Counter;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct DiagnosticCounts {
    pub workspaces: Counter,
    pub actors: Counter,
    pub tasks: Counter,
    pub runs: Counter,
    pub attempts: Counter,
    pub commands: Counter,
    pub events: Counter,
    pub checkpoints: Counter,
    pub manifests: Counter,
    pub blobs: Counter,
    pub erasure_fences: Counter,
}
#[derive(Debug, Serialize)]
pub struct LocalDiagnostics {
    pub schema_version: &'static str,
    pub journal_schema_version: i64,
    pub sqlite_version: &'static str,
    pub domain_tables: Counter,
    pub counts: DiagnosticCounts,
    pub execution_enabled: bool,
    pub profile: &'static str,
}
impl LocalStore {
    /// Requires existing exclusive store ownership. No payload/path/error export.
    pub async fn diagnostics(&mut self) -> Result<LocalDiagnostics, StoreError> {
        fn count(n: i64) -> Result<Counter, StoreError> {
            Counter::new(n.to_string())
                .map_err(|_| super::blobs::invalid("invalid diagnostic count"))
        }
        // One statement gives a consistent SQLite read snapshot. All names are
        // literals, and no caller can extend the report with a column or query.
        let values: (i64,i64,i64,i64,i64,i64,i64,i64,i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM local_workspaces), (SELECT count(*) FROM local_actors), (SELECT count(*) FROM local_tasks), (SELECT count(*) FROM local_runs), (SELECT count(*) FROM local_attempts), (SELECT count(*) FROM local_commands), (SELECT count(*) FROM local_events), (SELECT count(*) FROM local_checkpoints), (SELECT count(*) FROM local_manifests), (SELECT count(*) FROM local_blobs), (SELECT count(*) FROM local_erasure_fences)").fetch_one(&mut self.connection).await?;
        Ok(LocalDiagnostics {
            schema_version: "avencrew.local-diagnostics/1",
            journal_schema_version: self.status.schema_version,
            sqlite_version: super::ENGINE_VERSION,
            domain_tables: count(self.status.domain_tables as i64)?,
            counts: DiagnosticCounts {
                workspaces: count(values.0)?,
                actors: count(values.1)?,
                tasks: count(values.2)?,
                runs: count(values.3)?,
                attempts: count(values.4)?,
                commands: count(values.5)?,
                events: count(values.6)?,
                checkpoints: count(values.7)?,
                manifests: count(values.8)?,
                blobs: count(values.9)?,
                erasure_fences: count(values.10)?,
            },
            execution_enabled: false,
            profile: "local-storage-only",
        })
    }
    pub async fn diagnostic_bytes(&mut self) -> Result<Vec<u8>, StoreError> {
        let report = self.diagnostics().await?;
        let bytes = super::blobs::canonical(&report)?;
        if bytes.len() > 16 * 1024 {
            return Err(super::blobs::invalid("diagnostic size limit"));
        }
        Ok(bytes)
    }
}
#[cfg(test)]
mod tests {
    use super::super::commands::tests::{register, setup};
    use super::super::tests::{run, Root};
    use serde_json::{json, Value};
    #[test]
    fn diagnostics_export_only_fixed_metadata() {
        run(async {
            let root = Root::new();
            let mut store = setup(&root).await;
            register(&mut store).await.unwrap();
            // Credential locator and arbitrary user labels must not leak even though
            // retained database records contain them. The API never queries payloads.
            let sentinel = "PRIVATE_DIAGNOSTIC_SENTINEL";
            let id = |n: u32| {
                avencrew_contracts::scalars::DomainId::new(format!(
                    "01900000-0000-7000-8000-{n:012x}"
                ))
                .unwrap()
            };
            let payload=serde_json::to_vec(&json!({"kind":"steer","command_id":id(600),"run_id":id(300),"idempotency_key":"private-steer","payload":{"text":sentinel,"expected_revision":"1"}})).unwrap();
            let at =
                avencrew_contracts::scalars::Instant::new("2026-10-08T01:00:00.000000Z").unwrap();
            store
                .accept_command(crate::CommandAcceptance {
                    workspace: &id(1),
                    actor: &id(2),
                    bytes: &payload,
                    event: &id(601),
                    queue: &id(602),
                    command_blob: &id(603),
                    event_blob: &id(604),
                    at: &at,
                })
                .await
                .unwrap();
            sqlx::query("UPDATE local_actors SET display_name=?")
                .bind(sentinel)
                .execute(&mut store.connection)
                .await
                .unwrap();
            let bytes = store.diagnostic_bytes().await.unwrap();
            let value: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                value,
                json!({"schema_version":"avencrew.local-diagnostics/1","journal_schema_version":2,"sqlite_version":"3.53.4","domain_tables":"19","counts":{"workspaces":"1","actors":"1","tasks":"1","runs":"1","attempts":"0","commands":"1","events":"2","checkpoints":"0","manifests":"0","erasure_fences":"0","blobs":value["counts"]["blobs"]},"execution_enabled":false,"profile":"local-storage-only"})
            );
            let actual: i64 = sqlx::query_scalar("SELECT count(*) FROM local_blobs")
                .fetch_one(&mut store.connection)
                .await
                .unwrap();
            assert_eq!(value["counts"]["blobs"], actual.to_string());
            let text = String::from_utf8(bytes.clone()).unwrap();
            assert!(!text.contains(sentinel));
            assert!(!text.contains("keychain:test"));
            assert!(!text.contains("01900000"));
            assert!(!text.contains(&root.0.to_string_lossy().to_string()));
            assert_eq!(bytes, store.diagnostic_bytes().await.unwrap());
            store.close().await.unwrap();
        });
    }
}

#[cfg(test)]
mod boot_tests {
    use super::*;
    use crate::journal::tests::{run, Root};
    use std::fs;
    #[test]
    fn diagnostic_boot_never_creates_migrates_or_unquarantines() {
        run(async {
            let root = Root::new();
            assert!(LocalStore::open_existing_for_diagnostics(&root.0)
                .await
                .is_err());
            assert!(!root.0.join("execution.sqlite3").exists());
            LocalStore::open_to(&root.0, 1)
                .await
                .unwrap()
                .close()
                .await
                .unwrap();
            for _ in 0..20 {
                assert!(LocalStore::open_existing_for_diagnostics(&root.0)
                    .await
                    .is_err());
                // Rejection must finish closing SQLite before releasing the
                // writer lock. No background worker may retain its sidecars.
                assert!(!root.0.join("execution.sqlite3-wal").exists());
                assert!(!root.0.join("execution.sqlite3-shm").exists());
            }
            let mut old = LocalStore::open_to(&root.0, 1).await.unwrap();
            let version: i64 = sqlx::query_scalar("PRAGMA user_version")
                .fetch_one(&mut old.connection)
                .await
                .unwrap();
            assert_eq!(version, 1);
            old.close().await.unwrap();
            LocalStore::open(&root.0)
                .await
                .unwrap()
                .close()
                .await
                .unwrap();
            fs::write(root.0.join("restore-quarantine.json"), b"malformed marker").unwrap();
            assert!(LocalStore::open(&root.0).await.is_err());
            assert!(LocalStore::open_existing_for_diagnostics(&root.0)
                .await
                .is_err());
            fs::remove_file(root.0.join("restore-quarantine.json")).unwrap();
            std::os::unix::fs::symlink(
                root.0.join("nonexistent"),
                root.0.join("restore-quarantine.json"),
            )
            .unwrap();
            assert!(LocalStore::open(&root.0).await.is_err());
        });
    }
}
