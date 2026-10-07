use super::super::identity::tests::fixture as identity_fixture;
use super::super::tests::{run, Root};
use super::*;
use serde_json::{json, Value};
use std::fs;
fn id(n: u32) -> DomainId {
    DomainId::new(format!("01900000-0000-7000-8000-{n:012x}")).unwrap()
}
fn wrapper(table: &str, record: Value) -> Value {
    json!({"table":table,"id":record["id"],"record_schema":if table=="policy_versions" {"server-v1:target"} else {"server-v1:S0"},"record":record})
}
fn fixture(purpose: Purpose, raw: bool) -> Value {
    let task = purpose == Purpose::Task;
    let base = if task { 100 } else { 200 };
    let time = "2026-10-08T00:00:00.000000Z";
    let body = json!({"text":"Explicit outcome, acceptance criteria and evidence"});
    let content = if raw {
        digest(b"payload")
    } else {
        digest(&canonical(&body).unwrap())
    };
    let spec = json!({"schema_version":1,"success_criteria":"Human reviews the exact version against the requested outcome"});
    let resource = json!({"id":id(base+1),"workspace_id":id(1),"created_at":time,"row_version":"1","updated_at":time,"project_id":null,"owner_id":id(3),"kind":if task {"task"}else{"artifact"},"visibility":"private","status":"active","acl_generation":"1","current_version_id":id(base+2)});
    let version = json!({"id":id(base+2),"workspace_id":id(1),"created_at":time,"resource_id":id(base+1),"version_no":"1","blob_id":if raw {json!(id(base+6))}else{Value::Null},"inline_payload":if raw {Value::Null}else{body},"content_digest":hex(&content),"observed_at":time,"effective_at":null,"fresh_until":null,"source_etag":null,"created_by_id":id(3),"availability":"present","erased_at":null});
    let check = json!({"id":id(base+4),"workspace_id":id(1),"created_at":time,"resource_id":id(base+1),"version_no":1,"kind":"human","spec":spec,"digest":hex(&digest(&canonical(&spec).unwrap())),"required":true});
    let mut records = vec![
        wrapper("resources", resource),
        wrapper("resource_versions", version),
        wrapper("checks", check),
    ];
    if task {
        records.push(wrapper("tasks",json!({"id":id(base),"workspace_id":id(1),"created_at":time,"row_version":"1","updated_at":time,"resource_id":id(base+1),"owner_id":id(3),"current_revision_id":id(base+3),"status":"open","priority":0})));
        records.push(wrapper("task_revisions",json!({"id":id(base+3),"workspace_id":id(1),"created_at":time,"task_id":id(base),"version_no":1,"objective_version_id":id(base+2),"acting_principal_id":id(3),"audience_policy_id":id(6),"execution_policy_id":id(7),"completion_mode":"bounded","accepted_by_id":id(3),"accepted_at":time})));
        records.push(wrapper("task_checks",json!({"id":id(base+5),"workspace_id":id(1),"created_at":time,"task_revision_id":id(base+3),"check_id":id(base+4),"required":true})));
    } else {
        records.push(wrapper("artifacts",json!({"id":id(base),"workspace_id":id(1),"created_at":time,"row_version":"1","updated_at":time,"resource_id":id(base+1),"artifact_kind":"document","current_artifact_version_id":id(base+3),"status":"draft"})));
        records.push(wrapper("artifact_versions",json!({"id":id(base+3),"workspace_id":id(1),"created_at":time,"artifact_id":id(base),"resource_version_id":id(base+2),"base_version_id":null,"producing_run_id":null,"manifest_id":null,"editor_id":id(3)})));
        records.push(wrapper(
            "policy_versions",
            identity_fixture()["records"]["audience_policy"].clone(),
        ));
    }
    if raw {
        records.push(wrapper("blobs",json!({"id":id(base+6),"workspace_id":id(1),"created_at":time,"row_version":"1","updated_at":time,"owner_resource_id":id(base+1),"storage_key":"content/payload","sha256":hex(&content),"byte_size":"7","media_type":"text/plain","encryption_key_ref":null,"status":"verified","verified_at":time,"erased_at":null})));
    }
    json!({"schema_version":"1.0","purpose":if task {"task"}else{"artifact"},"root":{"table":if task {"tasks"}else{"artifacts"},"id":id(base)},"records":records,"referenced_blobs":if raw {json!([{"id":id(base+6),"sha256":hex(&content),"byte_size":"7"}])}else{json!([])}})
}
fn allocations(value: &Value) -> Vec<RecordAllocation> {
    let base = if value["purpose"] == "task" {
        1000
    } else {
        2000
    };
    value["records"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .filter(|(_, v)| !matches!(v["table"].as_str(), Some("principals" | "policy_versions")))
        .map(|(n, v)| RecordAllocation {
            table: v["table"].as_str().unwrap().into(),
            id: DomainId::new(v["id"].as_str().unwrap()).unwrap(),
            blob_id: id(base + n as u32),
        })
        .collect()
}
async fn setup(root: &Root) -> LocalStore {
    let mut store = LocalStore::open(&root.0).await.unwrap();
    store
        .bootstrap_standalone(
            &serde_json::to_vec(&identity_fixture()).unwrap(),
            "keychain:test",
        )
        .await
        .unwrap();
    store
}
async fn publish(
    store: &mut LocalStore,
    value: &Value,
    payloads: &[(DomainId, Vec<u8>)],
) -> Result<PublicationReceipt, StoreError> {
    let workspace = id(1);
    let actor = id(2);
    let bundle = id(if value["purpose"] == "task" { 900 } else { 901 });
    let bytes = canonical(value).unwrap();
    let allocations = allocations(value);
    let supplied: Vec<_> = payloads
        .iter()
        .map(|(id, bytes)| Payload { id, bytes })
        .collect();
    store
        .publish_bundle(BundlePublication {
            workspace: &workspace,
            actor: &actor,
            bundle_blob_id: &bundle,
            bytes: &bytes,
            sha256: digest(&bytes),
            record_blobs: &allocations,
            payloads: &supplied,
        })
        .await
}
fn field<'a>(value: &'a mut Value, table: &str) -> &'a mut Value {
    &mut value["records"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|v| v["table"] == table)
        .unwrap()["record"]
}
async fn count(store: &mut LocalStore, kind: &str) -> i64 {
    sqlx::query_scalar(if kind == "tasks" {
        "SELECT count(*) FROM local_tasks"
    } else {
        "SELECT count(*) FROM local_blobs"
    })
    .fetch_one(&mut store.connection)
    .await
    .unwrap()
}

#[test]
fn task_and_artifact_roundtrip_reopen_and_retry_derive_metadata() {
    run(async {
        let root = Root::new();
        let mut store = setup(&root).await;
        let task = fixture(Purpose::Task, false);
        let artifact = fixture(Purpose::Artifact, false);
        let t = publish(&mut store, &task, &[]).await.unwrap();
        let a = publish(&mut store, &artifact, &[]).await.unwrap();
        assert_eq!(
            store
                .read_task_bundle(&id(1), &id(2), &id(100))
                .await
                .unwrap()
                .canonical_bytes,
            canonical(&task).unwrap()
        );
        assert_eq!(
            store
                .read_artifact_bundle(&id(1), &id(2), &id(200))
                .await
                .unwrap()
                .canonical_bytes,
            canonical(&artifact).unwrap()
        );
        assert_eq!(publish(&mut store, &task, &[]).await.unwrap(), t);
        assert_eq!(publish(&mut store, &artifact, &[]).await.unwrap(), a);
        assert_eq!(count(&mut store, "tasks").await, 1);
        assert!(
            !sqlx::query("SELECT 1 FROM sqlite_schema WHERE name='local_artifacts'")
                .fetch_optional(&mut store.connection)
                .await
                .unwrap()
                .is_some()
        );
        store.close().await.unwrap();
        let mut reopened = LocalStore::open(&root.0).await.unwrap();
        assert_eq!(
            reopened
                .read_task_bundle(&id(1), &id(2), &id(100))
                .await
                .unwrap()
                .receipt,
            t
        );
        assert_eq!(
            reopened
                .read_artifact_bundle(&id(1), &id(2), &id(200))
                .await
                .unwrap()
                .receipt,
            a
        );
        assert!(reopened
            .read_task_bundle(&id(1), &id(3), &id(100))
            .await
            .is_err());
        reopened.close().await.unwrap();
    });
}

#[test]
fn invalid_digest_and_storage_allocations_publish_nothing() {
    run(async {
        let root = Root::new();
        let mut store = setup(&root).await;
        let task = fixture(Purpose::Task, false);
        let bytes = canonical(&task).unwrap();
        let workspace = id(1);
        let actor = id(2);
        let bundle = id(900);
        for case in 0..4 {
            let mut records = allocations(&task);
            let mut expected = digest(&bytes);
            match case {
                0 => expected[0] ^= 1,
                1 => {
                    records.pop();
                }
                2 => records[1].blob_id = records[0].blob_id.clone(),
                3 => records[0].blob_id = id(10), // retained registration blob
                _ => unreachable!(),
            }
            assert!(
                store
                    .publish_bundle(BundlePublication {
                        workspace: &workspace,
                        actor: &actor,
                        bundle_blob_id: &bundle,
                        bytes: &bytes,
                        sha256: expected,
                        record_blobs: &records,
                        payloads: &[],
                    })
                    .await
                    .is_err(),
                "invalid storage input case {case}"
            );
            assert_eq!(count(&mut store, "blobs").await, 7);
            assert_eq!(count(&mut store, "tasks").await, 0);
            assert!(!root
                .0
                .join("blobs")
                .join(id(1).as_str())
                .join("records")
                .exists());
        }
        store.close().await.unwrap();
    });
}

#[test]
fn raw_bytes_and_typed_declarations_agree_and_missing_content_blocks_reads() {
    run(async {
        let root = Root::new();
        let mut store = setup(&root).await;
        let task = fixture(Purpose::Task, true);
        assert!(publish(&mut store, &task, &[]).await.is_err());
        assert!(
            publish(&mut store, &task, &[(id(106), b"changed".to_vec())])
                .await
                .is_err()
        );
        assert_eq!(count(&mut store, "blobs").await, 7);
        let receipt = publish(&mut store, &task, &[(id(106), b"payload".to_vec())])
            .await
            .unwrap();
        assert_eq!(publish(&mut store, &task, &[]).await.unwrap(), receipt);
        assert_eq!(
            store
                .read_task_bundle(&id(1), &id(2), &id(100))
                .await
                .unwrap()
                .receipt,
            receipt
        );
        fs::write(root.0.join(relative(&id(1), &id(106))), b"altered").unwrap();
        assert!(store
            .read_task_bundle(&id(1), &id(2), &id(100))
            .await
            .is_err());
        assert!(publish(&mut store, &task, &[]).await.is_err());
        store.close().await.unwrap();
    });
}

#[test]
fn hostile_closures_fail_before_publishing_rows_or_files() {
    run(async {
        let root = Root::new();
        let mut store = setup(&root).await;
        let mut cases = Vec::new();
        for (table, name, value) in [
            ("resources", "owner_id", json!(id(99))),
            ("resources", "visibility", json!("workspace")),
            ("resources", "project_id", json!(id(99))),
            ("resources", "current_version_id", json!(id(99))),
            ("resource_versions", "resource_id", json!(id(99))),
            ("resource_versions", "content_digest", json!("0".repeat(64))),
            ("resource_versions", "availability", json!("erased")),
            ("tasks", "status", json!("completed")),
            ("tasks", "row_version", json!("2")),
            ("tasks", "current_revision_id", json!(id(99))),
            ("task_revisions", "task_id", json!(id(99))),
            ("task_revisions", "objective_version_id", json!(id(99))),
            ("task_revisions", "acting_principal_id", json!(id(99))),
            ("task_revisions", "audience_policy_id", json!(id(7))),
            ("task_revisions", "execution_policy_id", json!(id(6))),
            ("task_revisions", "accepted_by_id", json!(id(99))),
            ("task_checks", "check_id", json!(id(99))),
            ("checks", "resource_id", json!(id(99))),
            ("checks", "digest", json!("0".repeat(64))),
        ] {
            let mut value_ = fixture(Purpose::Task, false);
            field(&mut value_, table)[name] = value;
            cases.push(value_);
        }
        let mut missing = fixture(Purpose::Task, false);
        missing["records"]
            .as_array_mut()
            .unwrap()
            .retain(|r| r["table"] != "task_checks");
        cases.push(missing);
        let mut extra = fixture(Purpose::Task, false);
        let mut r = extra["records"][2].clone();
        r["id"] = json!(id(80));
        r["record"]["id"] = json!(id(80));
        extra["records"].as_array_mut().unwrap().push(r);
        cases.push(extra);
        let mut nullable = fixture(Purpose::Task, false);
        field(&mut nullable, "resource_versions")
            .as_object_mut()
            .unwrap()
            .remove("erased_at");
        cases.push(nullable);
        let mut unknown = fixture(Purpose::Task, false);
        field(&mut unknown, "tasks")["responsibility_id"] = Value::Null;
        cases.push(unknown);
        let mut authority = fixture(Purpose::Task, false);
        let mut principal = identity_fixture()["records"]["principal"].clone();
        principal["display_name"] = json!("Forged");
        authority["records"]
            .as_array_mut()
            .unwrap()
            .push(wrapper("principals", principal));
        cases.push(authority);
        for (table, name, value) in [
            ("artifacts", "status", json!("accepted")),
            ("artifact_versions", "artifact_id", json!(id(99))),
            ("artifact_versions", "resource_version_id", json!(id(99))),
            ("artifact_versions", "base_version_id", json!(id(99))),
            ("artifact_versions", "producing_run_id", json!(id(99))),
            ("artifact_versions", "editor_id", json!(id(99))),
        ] {
            let mut a = fixture(Purpose::Artifact, false);
            field(&mut a, table)[name] = value;
            cases.push(a);
        }
        let mut no_policy = fixture(Purpose::Artifact, false);
        no_policy["records"]
            .as_array_mut()
            .unwrap()
            .retain(|r| r["table"] != "policy_versions");
        cases.push(no_policy);
        for value in cases {
            assert!(
                publish(&mut store, &value, &[]).await.is_err(),
                "case {}",
                value
            );
        }
        for (field_name, value) in [
            ("owner_resource_id", json!(id(99))),
            ("storage_key", json!("../escape")),
            ("media_type", json!(super::super::blobs::REGISTRATION_MEDIA)),
        ] {
            let mut v = fixture(Purpose::Task, true);
            field(&mut v, "blobs")[field_name] = value;
            assert!(publish(&mut store, &v, &[(id(106), b"payload".to_vec())])
                .await
                .is_err());
        }
        assert_eq!(count(&mut store, "blobs").await, 7);
        assert_eq!(count(&mut store, "tasks").await, 0);
        assert!(!root
            .0
            .join("blobs")
            .join(id(1).as_str())
            .join("records")
            .exists());
        store.close().await.unwrap();
    });
}

#[test]
fn conflicting_versions_projections_and_registry_changes_are_refused() {
    run(async {
        let root = Root::new();
        let mut store = setup(&root).await;
        let original = fixture(Purpose::Task, false);
        publish(&mut store, &original, &[]).await.unwrap();
        let baseline = count(&mut store, "blobs").await;
        let mut changed = original.clone();
        field(&mut changed, "tasks")["priority"] = json!(10);
        assert!(publish(&mut store, &changed, &[]).await.is_err());
        assert_eq!(count(&mut store, "blobs").await, baseline);
        sqlx::query("UPDATE local_tasks SET row_version=2")
            .execute(&mut store.connection)
            .await
            .unwrap();
        assert!(store
            .read_task_bundle(&id(1), &id(2), &id(100))
            .await
            .is_err());
        assert!(publish(&mut store, &original, &[]).await.is_err());
        sqlx::query("UPDATE local_tasks SET row_version=1")
            .execute(&mut store.connection)
            .await
            .unwrap();
        sqlx::query("UPDATE local_actors SET status='disabled'")
            .execute(&mut store.connection)
            .await
            .unwrap();
        assert!(store
            .read_task_bundle(&id(1), &id(2), &id(100))
            .await
            .is_err());
        assert!(publish(&mut store, &original, &[]).await.is_err());
        store.close().await.unwrap();
    });
}

#[test]
fn real_transaction_abort_leaves_no_reference_then_retry_is_safe() {
    run(async {
        let root = Root::new();
        let mut store = setup(&root).await;
        sqlx::query("CREATE TEMP TRIGGER refuse_task BEFORE INSERT ON local_task_revisions BEGIN SELECT RAISE(ABORT,'test refusal'); END").execute(&mut store.connection).await.unwrap();
        let task = fixture(Purpose::Task, false);
        assert!(publish(&mut store, &task, &[]).await.is_err());
        assert_eq!(count(&mut store, "blobs").await, 7);
        assert_eq!(count(&mut store, "tasks").await, 0);
        assert!(store
            .read_task_bundle(&id(1), &id(2), &id(100))
            .await
            .is_err());
        sqlx::query("DROP TRIGGER refuse_task")
            .execute(&mut store.connection)
            .await
            .unwrap();
        publish(&mut store, &task, &[]).await.unwrap();
        store
            .read_task_bundle(&id(1), &id(2), &id(100))
            .await
            .unwrap();
        let previous = count(&mut store, "blobs").await;
        sqlx::query("CREATE TEMP TRIGGER refuse_artifact BEFORE INSERT ON local_blobs WHEN NEW.media_type='application/vnd.avencrew.record-bundle+json' BEGIN SELECT RAISE(ABORT,'test refusal'); END").execute(&mut store.connection).await.unwrap();
        let artifact = fixture(Purpose::Artifact, false);
        assert!(publish(&mut store, &artifact, &[]).await.is_err());
        assert_eq!(count(&mut store, "blobs").await, previous);
        assert!(store
            .read_artifact_bundle(&id(1), &id(2), &id(200))
            .await
            .is_err());
        sqlx::query("DROP TRIGGER refuse_artifact")
            .execute(&mut store.connection)
            .await
            .unwrap();
        publish(&mut store, &artifact, &[]).await.unwrap();
        store
            .read_artifact_bundle(&id(1), &id(2), &id(200))
            .await
            .unwrap();
        store.close().await.unwrap();
    });
}

#[test]
fn publication_crash_child() {
    if std::env::var("AVENCREW_PUBLICATION_CRASH_CHILD").as_deref() != Ok("1") {
        return;
    }
    let root = std::path::PathBuf::from(std::env::var("AVENCREW_PUBLICATION_CRASH_ROOT").unwrap());
    run(async {
        let mut store = LocalStore::open(&root).await.unwrap();
        let purpose =
            if std::env::var("AVENCREW_PUBLICATION_CRASH_PURPOSE").as_deref() == Ok("artifact") {
                Purpose::Artifact
            } else {
                Purpose::Task
            };
        publish(&mut store, &fixture(purpose, false), &[])
            .await
            .unwrap();
    });
}
#[test]
fn process_death_at_publication_commit_preserves_only_committed_receipts() {
    run(async {
        for (purpose, stage) in [
            (Purpose::Task, "before_commit"),
            (Purpose::Task, "after_commit"),
            (Purpose::Artifact, "before_commit"),
            (Purpose::Artifact, "after_commit"),
        ] {
            let root = Root::new();
            setup(&root).await.close().await.unwrap();
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "journal::publication::tests::publication_crash_child",
                    "--nocapture",
                ])
                .env("AVENCREW_PUBLICATION_CRASH_CHILD", "1")
                .env("AVENCREW_PUBLICATION_CRASH_STAGE", stage)
                .env(
                    "AVENCREW_PUBLICATION_CRASH_PURPOSE",
                    if purpose == Purpose::Task {
                        "task"
                    } else {
                        "artifact"
                    },
                )
                .env("AVENCREW_PUBLICATION_CRASH_ROOT", &root.0)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap();
            let start = std::time::Instant::now();
            while !root.0.join("publication-crash-ready").exists() {
                if let Some(status) = child.try_wait().unwrap() {
                    panic!("publication child exited early: {status}");
                }
                if start.elapsed() > std::time::Duration::from_secs(15) {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("publication child timed out");
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            assert_eq!(
                fs::read_to_string(root.0.join("publication-crash-ready")).unwrap(),
                stage
            );
            child.kill().unwrap();
            child.wait().unwrap();
            let mut store = LocalStore::open(&root.0).await.unwrap();
            let saved = if purpose == Purpose::Task {
                store.read_task_bundle(&id(1), &id(2), &id(100)).await
            } else {
                store.read_artifact_bundle(&id(1), &id(2), &id(200)).await
            };
            if stage == "before_commit" {
                assert_eq!(count(&mut store, "blobs").await, 7);
                assert_eq!(count(&mut store, "tasks").await, 0);
                assert!(saved.is_err());
            } else {
                assert!(saved.is_ok());
            }
            let receipt = publish(&mut store, &fixture(purpose, false), &[])
                .await
                .unwrap();
            let reopened_receipt = if purpose == Purpose::Task {
                store
                    .read_task_bundle(&id(1), &id(2), &id(100))
                    .await
                    .unwrap()
                    .receipt
            } else {
                store
                    .read_artifact_bundle(&id(1), &id(2), &id(200))
                    .await
                    .unwrap()
                    .receipt
            };
            assert_eq!(reopened_receipt, receipt);
            store.close().await.unwrap();
        }
    });
}
