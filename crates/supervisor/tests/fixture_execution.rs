//! Native supervisor process: fixture execution, real bytes and truthful limits.
use avencrew_contracts::{runtime::CheckStatus, scalars::DomainId};
use avencrew_store_sqlite::LocalStore;
use serde_json::{json, Value};
use std::{fs, io::Read, os::unix::fs::DirBuilderExt, path::PathBuf, process::Command};
struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("avencrew-fixture-cli-{}", uuid::Uuid::now_v7()));
        fs::DirBuilder::new().mode(0o700).create(&p).unwrap();
        Self(p)
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn input() -> Value {
    serde_json::from_str(include_str!("../../../fixtures/models/execution.json")).unwrap()
}
fn execute(root: &Root, value: &Value) -> std::process::Output {
    let inputs = Root::new();
    let file = inputs.0.join("input.json");
    fs::write(&file, serde_json::to_vec(value).unwrap()).unwrap();
    Command::new(env!("CARGO_BIN_EXE_avencrew-supervisor"))
        .args(["fixture-run", "--data-root"])
        .arg(&root.0)
        .arg("--input")
        .arg(file)
        .output()
        .unwrap()
}
fn runtime(f: impl std::future::Future<Output = ()>) {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(f)
}
#[test]
fn native_fixture_cli_retains_checked_bytes_and_never_overwrites_a_root() {
    let root = Root::new();
    let output = execute(&root, &input());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema_version"], "avencrew.fixture-run/1");
    assert_eq!(report["budget"]["model_calls"], "2");
    assert_eq!(report["budget"]["dispatches"], "1");
    assert_eq!(report["budget"]["known_input_tokens"], "50");
    assert_eq!(report["budget"]["known_output_tokens"], "25");
    assert_eq!(report["budget"]["unknown_model_calls"], "0");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("Synthetic export evidence"));
    runtime(async {
        let mut store = LocalStore::open(&root.0).await.unwrap();
        let id = |value: &Value| serde_json::from_value::<DomainId>(value.clone()).unwrap();
        let artifact = &report["outcome"]["artifact_refs"][0];
        let mut saved = store
            .read_local_artifact(
                &id(&report["workspace_id"]),
                &id(&report["actor_id"]),
                &id(&artifact["artifact_id"]),
                &id(&artifact["version_id"]),
            )
            .await
            .unwrap();
        assert_eq!(saved.validation.status, CheckStatus::Passed);
        assert_eq!(saved.pending_checks.len(), 1);
        let mut body = String::new();
        saved.file.read_to_string(&mut body).unwrap();
        assert!(body.contains("50001"));
        store.close().await.unwrap();
    });
    let before = fs::read(root.0.join("execution.sqlite3")).unwrap();
    let retry = execute(&root, &input());
    assert_eq!(retry.status.code(), Some(2));
    assert_eq!(fs::read(root.0.join("execution.sqlite3")).unwrap(), before);
}
#[test]
fn native_fixture_cli_rejects_paid_inputs_and_retains_actual_overruns() {
    let root = Root::new();
    let mut value = input();
    value["profile"]["rate_micro"] = json!("1");
    let output = execute(&root, &value);
    assert_eq!(output.status.code(), Some(2));
    assert!(!root.0.join("execution.sqlite3").exists());
    let root = Root::new();
    let mut value = input();
    for event in value["profile"]["episodes"][0]["input"]["events"]
        .as_array_mut()
        .unwrap()
    {
        if event["body"]["type"] == "usage" {
            event["body"]["input_tokens"] = json!("9000000");
        }
    }
    let output = execute(&root, &value);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["budget"]["known_input_tokens"], "9000000");
    assert_eq!(report["budget"]["overdrawn"], true);
    assert_eq!(report["budget"]["dispatches"], "0");
    assert_eq!(report["outcome"]["error"]["code"], "RESOURCE_LIMIT");
}

#[test]
fn native_fixture_cli_keeps_missing_usage_unknown() {
    let root = Root::new();
    let mut value = input();
    let events = value["profile"]["episodes"][0]["input"]["events"]
        .as_array_mut()
        .unwrap();
    events.retain(|e| e["body"]["type"] != "usage");
    for (n, event) in events.iter_mut().enumerate() {
        event["sequence"] = json!((n + 1).to_string());
    }
    let output = execute(&root, &value);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["budget"]["unknown_model_calls"], "1");
    assert_eq!(report["budget"]["known_input_tokens"], "30");
    assert_eq!(report["budget"]["known_output_tokens"], "15");
}

#[test]
fn native_fixture_cli_retains_interrupted_arguments_and_usage_without_dispatch() {
    let root = Root::new();
    let mut value = input();
    let events = value["profile"]["episodes"][0]["input"]["events"]
        .as_array_mut()
        .unwrap();
    events.retain(|e| {
        matches!(
            e["body"]["type"].as_str(),
            Some("tool_start" | "arguments_delta" | "usage")
        )
    });
    for (n, event) in events.iter_mut().enumerate() {
        event["sequence"] = json!((n + 1).to_string());
        if event["body"]["type"] == "arguments_delta" {
            event["body"]["bytes"].as_array_mut().unwrap().pop();
        }
    }
    let output = execute(&root, &value);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["budget"]["model_calls"], "1");
    assert_eq!(report["budget"]["tool_proposals"], "0");
    assert_eq!(report["budget"]["dispatches"], "0");
    assert_eq!(report["budget"]["known_input_tokens"], "20");
    assert_eq!(report["budget"]["known_output_tokens"], "10");
    assert_eq!(report["outcome"]["error"]["code"], "UNKNOWN_OUTCOME");
    assert_eq!(report["outcome"]["retry_class"], "reconcile_first");
}
