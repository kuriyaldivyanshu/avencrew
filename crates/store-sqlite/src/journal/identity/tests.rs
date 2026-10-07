use super::super::blobs::{relative, MAX_BYTES};
use super::super::tests::{run, Root};
use super::*;
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};

fn id(n: u32) -> String {
    format!("01900000-0000-7000-8000-{n:012x}")
}
fn domain(n: u32) -> DomainId {
    DomainId::new(id(n)).unwrap()
}
fn fixture() -> Value {
    let now = "2026-10-07T00:00:00.000000Z";
    let audience = json!({"allowed_principal_ids":[id(3)]});
    let execution = json!({"default":"deny","allowed_tools":[]});
    let records = json!({
        "user_reference":{"id":id(4),"created_at":now,"row_version":"1","updated_at":now,"display_name":"Human","status":"active","erased_at":null},
        "principal":{"id":id(3),"workspace_id":id(1),"created_at":now,"row_version":"1","updated_at":now,"kind":"human","user_id":id(4),"display_name":"Human","status":"active"},
        "workspace":{"id":id(1),"created_at":now,"row_version":"1","updated_at":now,"name":"Local","origin_device_key":"abcd","status":"active","permission_generation":"0","deletion_generation":"0","settings":{},"last_observation_seq":"0","last_sync_seq":"0"},
        "device":{"id":id(5),"workspace_id":id(1),"created_at":now,"row_version":"1","updated_at":now,"principal_id":id(3),"public_key":"abcd","label":"Device","os":"macos","architecture":"arm64","status":"active","last_seen_at":null,"minimum_deletion_generation":"0"},
        "audience_policy":{"id":id(6),"workspace_id":id(1),"created_at":now,"name":"Owner only","version_no":1,"policy_kind":"audience","schema_version":1,"rules":audience,"digest":hex(&digest(&canonical(&audience).unwrap())),"published_by_id":id(3)},
        "execution_policy":{"id":id(7),"workspace_id":id(1),"created_at":now,"name":"Deny dispatch","version_no":1,"policy_kind":"execution","schema_version":1,"rules":execution,"digest":hex(&digest(&canonical(&execution).unwrap())),"published_by_id":id(3)}
    });
    let mut refs = serde_json::Map::new();
    for (i, name) in [
        "user_reference",
        "principal",
        "workspace",
        "device",
        "audience_policy",
        "execution_policy",
    ]
    .into_iter()
    .enumerate()
    {
        let bytes = canonical(&records[name]).unwrap();
        refs.insert(name.into(),json!({"blob_id":id(20+i as u32),"record_id":records[name]["id"],"record_schema":"server-v1:target","sha256":hex(&digest(&bytes)),"byte_size":bytes.len().to_string()}));
    }
    json!({"registration":{"schema_version":"avencrew.local-registration/1.0","id":id(10),"workspace_id":id(1),"actor_id":id(2),"device_id":id(5),"principal_id":id(3),"user_reference_id":id(4),"audience_policy_id":id(6),"execution_policy_id":id(7),"created_at":now,"origin_public_key":"abcd","permission_generation":"0","deletion_generation":"0","records":refs},"records":records})
}
fn encoded() -> Vec<u8> {
    serde_json::to_vec(&fixture()).unwrap()
}
async fn rows(store: &mut LocalStore, table: &str) -> i64 {
    // Only test-owned fixed table names, never a public SQL interface.
    sqlx::query_scalar(match table {
        "workspace" => "SELECT count(*) FROM local_workspaces",
        "actor" => "SELECT count(*) FROM local_actors",
        "device" => "SELECT count(*) FROM local_devices",
        _ => "SELECT count(*) FROM local_blobs",
    })
    .fetch_one(&mut store.connection)
    .await
    .unwrap()
}

#[test]
fn registration_reopens_retries_and_matches_typed_projections() {
    run(async {
        let root = Root::new();
        let mut store = LocalStore::open(&root.0).await.unwrap();
        let receipt = store
            .bootstrap_standalone(&encoded(), "keychain:local-test")
            .await
            .unwrap();
        assert_eq!(receipt.actor_id, domain(2));
        assert_eq!(rows(&mut store, "blobs").await, 7);
        assert_eq!(
            store
                .bootstrap_standalone(&encoded(), "keychain:local-test")
                .await
                .unwrap(),
            receipt
        );
        assert!(store
            .bootstrap_standalone(&encoded(), "keychain:changed")
            .await
            .is_err());
        assert_eq!(
            store
                .resolve_standalone(&domain(1), &domain(2))
                .await
                .unwrap(),
            receipt
        );
        for entry in fs::read_dir(root.0.join("blobs").join(id(1))).unwrap() {
            let path = entry.unwrap().path();
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        store.close().await.unwrap();
        let mut reopened = LocalStore::open(&root.0).await.unwrap();
        assert_eq!(
            reopened
                .resolve_standalone(&domain(1), &domain(2))
                .await
                .unwrap(),
            receipt
        );
        assert_eq!(
            reopened
                .bootstrap_standalone(&encoded(), "keychain:local-test")
                .await
                .unwrap(),
            receipt
        );
        assert!(reopened
            .resolve_standalone(&domain(1), &domain(3))
            .await
            .is_err());
        let mut changed = fixture();
        changed["registration"]["id"] = json!(id(11));
        assert!(reopened
            .bootstrap_standalone(
                &serde_json::to_vec(&changed).unwrap(),
                "keychain:local-test"
            )
            .await
            .is_err());
        assert_eq!(rows(&mut reopened, "blobs").await, 7);
        reopened.close().await.unwrap();
    });
}

#[test]
fn hostile_shapes_mappings_and_policy_widening_write_nothing() {
    run(async {
        let mut cases = Vec::new();
        for pointer in [
            "/records/user_reference/erased_at",
            "/records/principal/user_id",
            "/records/workspace/origin_device_key",
            "/records/device/last_seen_at",
        ] {
            let mut v = fixture();
            let (parent, name) = pointer.rsplit_once('/').unwrap();
            v.pointer_mut(parent)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(name);
            cases.push(v);
        }
        for (pointer, value) in [
            ("/records/principal/kind", json!("worker")),
            ("/records/principal/user_id", json!(id(99))),
            ("/records/device/workspace_id", json!(id(99))),
            ("/records/device/public_key", json!("different-key")),
            (
                "/records/audience_policy/rules/allowed_principal_ids",
                json!([id(3), id(99)]),
            ),
            ("/records/execution_policy/rules/default", json!("allow")),
            (
                "/records/execution_policy/rules/allowed_tools",
                json!(["exec_command"]),
            ),
            ("/records/audience_policy/schema_version", json!("1")),
            (
                "/registration/records/principal/sha256",
                json!("0".repeat(64)),
            ),
            ("/registration/records/principal/byte_size", json!("1")),
            ("/registration/records/principal/blob_id", json!(id(10))),
            ("/registration/origin_public_key", json!("abc")),
            ("/records/user_reference/row_version", json!("0")),
            ("/records/workspace/settings", json!({"allow_all":true})),
        ] {
            let mut v = fixture();
            *v.pointer_mut(pointer).unwrap() = value;
            cases.push(v);
        }
        let mut extra = fixture();
        extra["records"]["principal"]["grant"] = json!("admin");
        cases.push(extra);
        let root = Root::new();
        let mut store = LocalStore::open(&root.0).await.unwrap();
        for value in cases {
            assert!(store
                .bootstrap_standalone(&serde_json::to_vec(&value).unwrap(), "keychain:test")
                .await
                .is_err());
        }
        assert!(store
            .bootstrap_standalone(&vec![b' '; MAX_BYTES + 1], "keychain:test")
            .await
            .is_err());
        assert!(store
            .bootstrap_standalone(br#"{"registration":{},"registration":{}}"#, "keychain:test")
            .await
            .is_err());
        assert!(store.bootstrap_standalone(&encoded(), "").await.is_err());
        for table in ["workspace", "actor", "device", "blobs"] {
            assert_eq!(rows(&mut store, table).await, 0);
        }
        assert!(!root.0.join("blobs").exists());
        store.close().await.unwrap();
    });
}

#[test]
fn database_abort_publishes_no_registry_and_retry_uses_verified_orphans() {
    run(async {
        let root = Root::new();
        let mut store = LocalStore::open(&root.0).await.unwrap();
        sqlx::query("CREATE TEMP TRIGGER refuse_registration BEFORE INSERT ON local_blobs BEGIN SELECT RAISE(ABORT,'test refusal'); END").execute(&mut store.connection).await.unwrap();
        assert!(store
            .bootstrap_standalone(&encoded(), "keychain:test")
            .await
            .is_err());
        for table in ["workspace", "actor", "device", "blobs"] {
            assert_eq!(rows(&mut store, table).await, 0);
        }
        assert!(store
            .resolve_standalone(&domain(1), &domain(2))
            .await
            .is_err());
        assert_eq!(
            fs::read_dir(root.0.join("blobs").join(id(1)))
                .unwrap()
                .count(),
            7
        );
        sqlx::query("DROP TRIGGER refuse_registration")
            .execute(&mut store.connection)
            .await
            .unwrap();
        let receipt = store
            .bootstrap_standalone(&encoded(), "keychain:test")
            .await
            .unwrap();
        assert_eq!(
            store
                .resolve_standalone(&domain(1), &domain(2))
                .await
                .unwrap(),
            receipt
        );
        store.close().await.unwrap();
    });
}

#[test]
fn changed_missing_linked_or_permissive_payloads_block_resolution() {
    run(async {
        for mode in [
            "changed",
            "missing",
            "symlink",
            "hardlink",
            "directory",
            "metadata",
        ] {
            let root = Root::new();
            let mut store = LocalStore::open(&root.0).await.unwrap();
            store
                .bootstrap_standalone(&encoded(), "keychain:test")
                .await
                .unwrap();
            let path = root.0.join(relative(&domain(1), &domain(21)));
            match mode {
                "changed" => fs::write(&path, b"{}").unwrap(),
                "missing" => fs::remove_file(&path).unwrap(),
                "symlink" => {
                    fs::remove_file(&path).unwrap();
                    symlink(root.0.join(relative(&domain(1), &domain(20))), &path).unwrap();
                }
                "hardlink" => fs::hard_link(&path, root.0.join("duplicate")).unwrap(),
                "directory" => {
                    fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o755))
                        .unwrap()
                }
                "metadata" => {
                    sqlx::query("UPDATE local_blobs SET storage_relpath='elsewhere' WHERE workspace_id=? AND id=?").bind(id(1)).bind(id(21)).execute(&mut store.connection).await.unwrap();
                }
                _ => unreachable!(),
            }
            assert!(
                store
                    .resolve_standalone(&domain(1), &domain(2))
                    .await
                    .is_err(),
                "mode {mode}"
            );
            store.close().await.unwrap();
        }
    });
}

#[test]
fn live_revocation_generations_expiry_and_erasure_block_resolution() {
    run(async {
        for mutation in [
            "UPDATE local_workspaces SET permission_generation=1",
            "UPDATE local_workspaces SET deletion_generation=1",
            "UPDATE local_actors SET status='disabled'",
            "UPDATE local_actors SET authority_expires_at_us=1",
            "UPDATE local_devices SET status='revoked'",
            "UPDATE local_blobs SET status='quarantined'",
            "UPDATE local_blobs SET status='erased',erased_at_us=1",
        ] {
            let root = Root::new();
            let mut store = LocalStore::open(&root.0).await.unwrap();
            store
                .bootstrap_standalone(&encoded(), "keychain:test")
                .await
                .unwrap();
            sqlx::query(mutation)
                .execute(&mut store.connection)
                .await
                .unwrap();
            assert!(
                store
                    .resolve_standalone(&domain(1), &domain(2))
                    .await
                    .is_err(),
                "mutation {mutation}"
            );
            assert!(store
                .bootstrap_standalone(&encoded(), "keychain:test")
                .await
                .is_err());
            store.close().await.unwrap();
        }
        let root = Root::new();
        let mut store = LocalStore::open(&root.0).await.unwrap();
        store
            .bootstrap_standalone(&encoded(), "keychain:test")
            .await
            .unwrap();
        sqlx::query("INSERT INTO local_erasure_fences VALUES (?,?,0,1,?,'fenced',NULL)")
            .bind(id(90))
            .bind(id(1))
            .bind(id(20))
            .execute(&mut store.connection)
            .await
            .unwrap();
        assert!(store
            .resolve_standalone(&domain(1), &domain(2))
            .await
            .is_err());
        store.close().await.unwrap();
    });
}

#[test]
fn suppression_is_checked_before_opening_retained_content() {
    run(async {
        let root = Root::new();
        let mut store = LocalStore::open(&root.0).await.unwrap();
        store
            .bootstrap_standalone(&encoded(), "keychain:test")
            .await
            .unwrap();
        fs::remove_file(root.0.join(relative(&domain(1), &domain(10)))).unwrap();
        sqlx::query("UPDATE local_actors SET status='disabled'")
            .execute(&mut store.connection)
            .await
            .unwrap();
        assert!(matches!(
            store.resolve_standalone(&domain(1), &domain(2)).await,
            Err(StoreError::Incompatible(
                "standalone identity unavailable before content lookup"
            ))
        ));
        store.close().await.unwrap();
    });
}

#[test]
fn canonical_publication_never_overwrites_and_recovers_own_staging_names() {
    run(async {
        let root = Root::new();
        let store = LocalStore::open(&root.0).await.unwrap();
        let path = store.put_canonical(&domain(1), &domain(20), b"{}").unwrap();
        assert!(store.put_canonical(&domain(1), &domain(20), b"[]").is_err());
        assert_eq!(fs::read(root.0.join(path)).unwrap(), b"{}");
        let pending = root
            .0
            .join("blobs")
            .join(id(1))
            .join(format!("{}.pending", id(20)));
        fs::hard_link(root.0.join(relative(&domain(1), &domain(20))), &pending).unwrap();
        store.put_canonical(&domain(1), &domain(20), b"{}").unwrap();
        assert!(!pending.exists());
        store.close().await.unwrap();
    });
}

#[test]
fn registration_crash_child() {
    if std::env::var("AVENCREW_IDENTITY_CRASH_CHILD").as_deref() != Ok("1") {
        return;
    }
    let root = std::path::PathBuf::from(std::env::var("AVENCREW_IDENTITY_CRASH_ROOT").unwrap());
    run(async {
        let mut store = LocalStore::open(&root).await.unwrap();
        store
            .bootstrap_standalone(&encoded(), "keychain:test")
            .await
            .unwrap();
    });
}

#[test]
fn process_death_before_and_after_real_registration_commit_reconciles() {
    run(async {
        for stage in ["before_commit", "after_commit"] {
            let root = Root::new();
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "journal::identity::tests::registration_crash_child",
                    "--nocapture",
                ])
                .env("AVENCREW_IDENTITY_CRASH_CHILD", "1")
                .env("AVENCREW_IDENTITY_CRASH_STAGE", stage)
                .env("AVENCREW_IDENTITY_CRASH_ROOT", &root.0)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap();
            let start = std::time::Instant::now();
            while !root.0.join("identity-crash-ready").exists() {
                if let Some(status) = child.try_wait().unwrap() {
                    panic!("registration child exited early: {status}");
                }
                if start.elapsed() > std::time::Duration::from_secs(15) {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("registration child did not reach commit barrier");
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            assert_eq!(
                fs::read_to_string(root.0.join("identity-crash-ready")).unwrap(),
                stage
            );
            child.kill().unwrap();
            child.wait().unwrap();
            let mut reopened = LocalStore::open(&root.0).await.unwrap();
            let expected = Bootstrap::decode(&encoded()).unwrap().receipt().unwrap();
            if stage == "before_commit" {
                for table in ["workspace", "actor", "device", "blobs"] {
                    assert_eq!(rows(&mut reopened, table).await, 0);
                }
                assert!(reopened
                    .resolve_standalone(&domain(1), &domain(2))
                    .await
                    .is_err());
            } else {
                assert_eq!(rows(&mut reopened, "blobs").await, 7);
                assert_eq!(
                    reopened
                        .resolve_standalone(&domain(1), &domain(2))
                        .await
                        .unwrap(),
                    expected
                );
            }
            // Before commit, reuse the unreferenced verified bytes. After commit,
            // reconcile the existing receipt rather than allocating a new ID.
            assert_eq!(
                reopened
                    .bootstrap_standalone(&encoded(), "keychain:test")
                    .await
                    .unwrap(),
                expected
            );
            assert_eq!(rows(&mut reopened, "blobs").await, 7);
            reopened.close().await.unwrap();
        }
    });
}
