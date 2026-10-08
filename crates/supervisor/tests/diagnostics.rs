//! Real CLI output and no-init/error privacy checks.
use std::{fs, os::unix::fs::DirBuilderExt, process::Command};
#[test]
fn diagnostic_cli_is_metadata_only_and_refuses_missing_root() {
    let root = std::env::temp_dir().join(format!("avencrew-diagnostic-cli-{}", std::process::id()));
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    let binary = env!("CARGO_BIN_EXE_avencrew-supervisor");
    let output = Command::new(binary)
        .args(["diagnostics", "--data-root"])
        .arg(root.join("PRIVATE_MISSING_ROOT"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(!root.join("PRIVATE_MISSING_ROOT").exists());
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "diagnostics: verified local store unavailable\n"
    );
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            avencrew_store_sqlite::LocalStore::open(&root)
                .await
                .unwrap()
                .close()
                .await
                .unwrap();
        });
    let output = Command::new(binary)
        .args(["diagnostics", "--data-root"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let v: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(v["schema_version"], "avencrew.local-diagnostics/1");
    assert_eq!(v["execution_enabled"], false);
    assert_eq!(v["counts"]["runs"], "0");
    assert!(!String::from_utf8(output.stdout)
        .unwrap()
        .contains(&root.to_string_lossy().to_string()));
    fs::write(root.join("restore-quarantine.json"), b"unverified").unwrap();
    let output = Command::new(binary)
        .args(["diagnostics", "--data-root"])
        .arg(&root)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
}
