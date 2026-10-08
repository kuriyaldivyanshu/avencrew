use super::super::tests::{register, setup};
use super::*;
use crate::journal::tests::{run, Root};
use crate::BlobImport;
use serde_json::{json, Value};
use std::{
    fs,
    io::Read,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant as Clock},
};
fn id(n: u32) -> DomainId {
    DomainId::new(format!("01900000-0000-7000-8000-{n:012x}")).unwrap()
}
fn at() -> Instant {
    Instant::new("2026-10-08T01:00:00.000000Z").unwrap()
}
pub(crate) async fn ready(root: &Root) -> LocalStore {
    let mut s = setup(root).await;
    register(&mut s).await.unwrap();
    sqlx::query("UPDATE local_runs SET status='preparing',fencing_epoch=1 WHERE id=?")
        .bind(id(300).as_str())
        .execute(&mut s.connection)
        .await
        .unwrap();
    sqlx::query("INSERT INTO local_attempts (id,workspace_id,created_at_us,run_id,epoch,placement,state,harness_build_digest,protocol_version,lease_receipt_blob_id,lease_expires_at_us,monotonic_deadline_ns,boot_id,started_at_us,ended_at_us) VALUES (?,?,?, ?,1,'local','claimed',?,1,NULL,NULL,NULL,'fixture-boot',?,NULL)").bind(id(310).as_str()).bind(id(1).as_str()).bind(at().unix_micros()).bind(id(300).as_str()).bind("a".repeat(64)).bind(at().unix_micros()).execute(&mut s.connection).await.unwrap();
    let bytes = b"captured file\n";
    s.import_vault_blob(
        BlobImport {
            workspace: &id(1),
            actor: &id(2),
            id: &id(320),
            sha256: &avencrew_contracts::scalars::Digest::new(hex(&digest(bytes))).unwrap(),
            byte_size: &counter(bytes.len() as i64).unwrap(),
            at: &at(),
        },
        &bytes[..],
    )
    .await
    .unwrap();
    s
}
pub(crate) fn fixture() -> Value {
    let mut entries = vec![
        json!({"id":id(402),"workspace_id":id(1),"created_at":at(),"manifest_id":id(401),"entry_key":"coverage","kind":"metadata","blob_id":null,"resource_version_id":null,"invocation_id":null,"process_id":null,"child_run_id":null,"credential_reference_id":null,"sha256":null,"byte_size":null,"mode_bits":null,"disposition":"declared","metadata":{"schema_version":"avencrew.checkpoint-coverage/1","capture":"per_file","filesystem_event_seq":"1","gaps":["No complete tree snapshot; context, effects and processes require reconciliation"],"context":"not_captured","effects":"unverified","processes":"unverified","attempt_epoch":"1","harness_build_digest":"a".repeat(64),"protocol_version":1}}),
    ];
    let mut file = entries[0].clone();
    file["id"] = json!(id(403));
    file["entry_key"] = json!("file/src/example.txt");
    file["kind"] = json!("file");
    file["blob_id"] = json!(id(320));
    file["sha256"] = json!(hex(&digest(b"captured file\n")));
    file["byte_size"] = json!("14");
    file["mode_bits"] = json!(0o644);
    file["disposition"] = json!("captured");
    file["metadata"] = json!({});
    entries.push(file);
    let m = json!({"id":id(401),"workspace_id":id(1),"created_at":at(),"owner_resource_id":id(101),"kind":"checkpoint","schema_version":1,"digest":hex(&digest(&canonical(&entries).unwrap())),"sealed_at":at(),"deletion_generation":"0"});
    let mut c = json!({"id":id(400),"workspace_id":id(1),"created_at":at(),"run_id":id(300),"attempt_id":id(310),"event_seq":"1","accepted_command_seq":"0","applied_command_seq":"0","manifest_id":id(401),"context_manifest_id":null,"checkpoint_kind":"filesystem","schema_version":1,"sealed_at":at()});
    c["digest"]=json!(hex(&digest(&canonical(&json!({"domain":"avencrew.local-checkpoint-record/1","record":c,"manifest_digest":m["digest"]})).unwrap())));
    let wrap = |table: &str, record: Value| json!({"table":table,"id":record["id"],"record_schema":if table=="checkpoints"{"server-v1:S0"}else{"server-v1:target"},"record":record});
    let mut records = vec![wrap("checkpoints", c), wrap("manifests", m)];
    records.extend(entries.into_iter().map(|e| wrap("manifest_entries", e)));
    json!({"schema_version":"1.0","purpose":"checkpoint","root":{"table":"checkpoints","id":id(400)},"records":records,"referenced_blobs":[{"id":id(320),"sha256":hex(&digest(b"captured file\n")),"byte_size":"14"}]})
}
fn rehash(v: &mut Value) {
    let entries: Vec<Value> = v["records"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["table"] == "manifest_entries")
        .map(|r| r["record"].clone())
        .collect();
    v["records"][1]["record"]["digest"] = json!(hex(&digest(&canonical(&entries).unwrap())));
    let mut c = v["records"][0]["record"].clone();
    c.as_object_mut().unwrap().remove("digest");
    v["records"][0]["record"]["digest"]=json!(hex(&digest(&canonical(&json!({"domain":"avencrew.local-checkpoint-record/1","record":c,"manifest_digest":v["records"][1]["record"]["digest"]})).unwrap())));
}
pub(crate) async fn publish(
    s: &mut LocalStore,
    v: &Value,
) -> Result<CheckpointReceipt, ControlError> {
    let bytes = canonical(v).unwrap();
    s.publish_checkpoint(CheckpointPublication {
        workspace: &id(1),
        actor: &id(2),
        bytes: &bytes,
        sha256: digest(&bytes),
        bundle_blob: &id(410),
        event: &id(411),
        event_blob: &id(412),
        expected_version: &positive(1).unwrap(),
    })
    .await
}
#[test]
fn atomic_checkpoint_reopen_retry_and_recovery_hold() {
    run(async {
        let root = Root::new();
        let mut s = ready(&root).await;
        let v = fixture();
        let r = publish(&mut s, &v).await.unwrap();
        assert_eq!(r.event_seq.value(), 2);
        assert_eq!(r.row_version.value(), 2);
        assert_eq!(publish(&mut s, &v).await.unwrap(), r);
        let row = sqlx::query("SELECT * FROM local_runs")
            .fetch_one(&mut s.connection)
            .await
            .unwrap();
        assert_eq!(
            row.get::<String, _>("current_checkpoint_id"),
            id(400).as_str()
        );
        assert_eq!(row.get::<i64, _>("accepted_command_seq"), 0);
        assert_eq!(row.get::<i64, _>("applied_command_seq"), 0);
        assert_eq!(row.get::<i64, _>("fencing_epoch"), 1);
        s.close().await.unwrap();
        let mut s = LocalStore::open(&root.0).await.unwrap();
        let mut restored = s
            .read_checkpoint(&id(1), &id(2), &id(300), &id(400))
            .await
            .unwrap();
        assert_eq!(restored.receipt, r);
        assert_eq!(restored.coverage.effects, "unverified");
        assert!(!restored.coverage.gaps.is_empty());
        assert_eq!(restored.files[0].path, "src/example.txt");
        let mut bytes = Vec::new();
        restored.files[0].file.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"captured file\n");
        let recovery = s.recover_local_controls(&id(1), &id(2)).await.unwrap();
        assert_eq!(recovery.runs[0].gate, RecoveryGate::ReconciliationRequired);
        fs::write(
            root.0.join(format!(
                "blobs/{}/vault/{}.bin",
                id(1).as_str(),
                id(320).as_str()
            )),
            b"tampered file\n",
        )
        .unwrap();
        assert!(s
            .read_checkpoint(&id(1), &id(2), &id(300), &id(400))
            .await
            .is_err());
        assert!(s.recover_local_controls(&id(1), &id(2)).await.is_err());
        s.close().await.unwrap();
    });
}
#[test]
fn checkpoint_rejects_invalid_closure_authority_and_rollback() {
    run(async {
        let root = Root::new();
        let mut s = ready(&root).await;
        for (index, key, value) in [
            (3, "entry_key", json!("file/../escape")),
            (2, "metadata", json!({})),
            (0, "context_manifest_id", json!(id(999))),
            (0, "event_seq", json!("2")),
            (0, "attempt_id", json!(id(999))),
            (1, "owner_resource_id", json!(id(999))),
        ] {
            let mut v = fixture();
            v["records"][index]["record"][key] = value;
            rehash(&mut v);
            assert!(publish(&mut s, &v).await.is_err(), "{key}");
        }
        sqlx::query("UPDATE local_runs SET cancellation_requested=1,status='cancelling'")
            .execute(&mut s.connection)
            .await
            .unwrap();
        assert!(matches!(
            publish(&mut s, &fixture()).await,
            Err(ControlError::Cancelled)
        ));
        s.close().await.unwrap();
        let root = Root::new();
        let mut s = ready(&root).await;
        sqlx::query("CREATE TEMP TRIGGER refuse_checkpoint BEFORE INSERT ON local_checkpoints BEGIN SELECT RAISE(ABORT,'fixture rollback'); END").execute(&mut s.connection).await.unwrap();
        assert!(publish(&mut s, &fixture()).await.is_err());
        for sql in [
            "SELECT count(*) FROM local_checkpoints",
            "SELECT count(*) FROM local_manifests",
        ] {
            let n: i64 = sqlx::query_scalar(sql)
                .fetch_one(&mut s.connection)
                .await
                .unwrap();
            assert_eq!(n, 0);
        }
        let row = run_row(&mut s.connection, &id(1), &id(300), &id(2))
            .await
            .unwrap();
        assert_eq!((row.event, row.version), (1, 1));
        sqlx::query("DROP TRIGGER refuse_checkpoint")
            .execute(&mut s.connection)
            .await
            .unwrap();
        publish(&mut s, &fixture()).await.unwrap();
        s.close().await.unwrap();
    });
}
pub(super) fn commit_barrier(database: &Path, stage: &str) -> Result<(), StoreError> {
    let marker = database
        .parent()
        .unwrap()
        .join(format!("checkpoint-{stage}"));
    if marker.exists() {
        fs::write(marker.with_extension("reached"), b"ready")?;
        loop {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    Ok(())
}
#[test]
fn checkpoint_crash_child() {
    let Ok(path) = std::env::var("AVENCREW_CHECKPOINT_CRASH_ROOT") else {
        return;
    };
    run(async {
        let mut s = LocalStore::open(Path::new(&path)).await.unwrap();
        publish(&mut s, &fixture()).await.unwrap();
    });
}
#[test]
fn checkpoint_kill_at_commit_has_one_atomic_cut() {
    for stage in ["before_commit", "after_commit"] {
        let root = Root::new();
        run(async {
            ready(&root).await.close().await.unwrap();
        });
        fs::write(root.0.join(format!("checkpoint-{stage}")), b"wait").unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "journal::commands::checkpoints::tests::checkpoint_crash_child",
                "--nocapture",
            ])
            .env("AVENCREW_CHECKPOINT_CRASH_ROOT", &root.0)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let reached = root.0.join(format!("checkpoint-{stage}.reached"));
        let start = Clock::now();
        while !reached.exists() {
            if start.elapsed() > Duration::from_secs(15) {
                let _ = child.kill();
                panic!("checkpoint child did not reach {stage}");
            }
            assert!(child.try_wait().unwrap().is_none());
            std::thread::sleep(Duration::from_millis(10));
        }
        child.kill().unwrap();
        child.wait().unwrap();
        fs::remove_file(root.0.join(format!("checkpoint-{stage}"))).unwrap();
        run(async {
            let mut s = LocalStore::open(&root.0).await.unwrap();
            let n: i64 = sqlx::query_scalar("SELECT count(*) FROM local_checkpoints")
                .fetch_one(&mut s.connection)
                .await
                .unwrap();
            assert_eq!(n, if stage == "before_commit" { 0 } else { 1 });
            let r = publish(&mut s, &fixture()).await.unwrap();
            assert_eq!(r.event_seq.value(), 2);
            s.close().await.unwrap();
        });
    }
}

#[test]
fn checkpoint_compatibility_and_unavailable_bytes_fail_closed() {
    run(async {
        let root = Root::new();
        let mut s = ready(&root).await;
        for (field, value) in [
            ("attempt_epoch", json!("2")),
            ("harness_build_digest", json!("b".repeat(64))),
            ("protocol_version", json!(2)),
            ("gaps", json!([])),
            ("effects", json!("resolved")),
        ] {
            let mut v = fixture();
            v["records"][2]["record"]["metadata"][field] = value;
            rehash(&mut v);
            assert!(publish(&mut s, &v).await.is_err(), "{field}");
        }
        let mut v = fixture();
        v["referenced_blobs"] = json!([]);
        assert!(publish(&mut s, &v).await.is_err());
        sqlx::query("UPDATE local_attempts SET state='released',ended_at_us=?")
            .bind(at().unix_micros())
            .execute(&mut s.connection)
            .await
            .unwrap();
        assert!(publish(&mut s, &fixture()).await.is_err());
        sqlx::query("UPDATE local_attempts SET state='claimed',ended_at_us=NULL")
            .execute(&mut s.connection)
            .await
            .unwrap();
        sqlx::query("UPDATE local_runs SET authority='server',authority_device_id=NULL")
            .execute(&mut s.connection)
            .await
            .unwrap();
        assert!(matches!(
            publish(&mut s, &fixture()).await,
            Err(ControlError::StaleAuthority)
        ));
        // Device allocation comes from the verified registry, never a fabricated ID.
        let identity = s.verified_identity(&id(1), &id(2)).await.unwrap();
        sqlx::query("UPDATE local_runs SET authority='local',authority_device_id=?")
            .bind(identity.device().as_str())
            .execute(&mut s.connection)
            .await
            .unwrap();
        publish(&mut s, &fixture()).await.unwrap();
        assert!(s
            .read_checkpoint(&id(1), &id(3), &id(300), &id(400))
            .await
            .is_err());
        fs::remove_file(root.0.join(format!(
            "blobs/{}/vault/{}.bin",
            id(1).as_str(),
            id(320).as_str()
        )))
        .unwrap();
        assert!(s
            .read_checkpoint(&id(1), &id(2), &id(300), &id(400))
            .await
            .is_err());
        assert!(s.recover_local_controls(&id(1), &id(2)).await.is_err());
        s.close().await.unwrap();
    });
}
