//! Real offline backup/inspection/quarantined-copy command grammar and outputs.
use std::{fs, os::unix::fs::DirBuilderExt, process::Command};
#[test]
fn cli_backup_and_restore_never_activate_saved_state() {
    let parent = std::env::temp_dir().join(format!("avencrew-backup-cli-{}", std::process::id()));
    fs::DirBuilder::new().mode(0o700).create(&parent).unwrap();
    let parent = parent.canonicalize().unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(parent.clone());
    let root = parent.join("source");
    let package = parent.join("package");
    let dest = parent.join("restored");
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
    let binary = env!("CARGO_BIN_EXE_avencrew-supervisor");
    let output = Command::new(binary)
        .args(["backup", "--data-root"])
        .arg(&root)
        .arg("--destination")
        .arg(&package)
        .args([
            "--backup-id",
            "01900000-0000-7000-8000-000000000800",
            "--at",
            "2026-10-08T01:00:00.000000Z",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let backed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(backed["disposition"], "verified_private_backup");
    assert!(output.stderr.is_empty());
    let checked = Command::new(binary)
        .args(["backup-check", "--source"])
        .arg(&package)
        .output()
        .unwrap();
    assert!(checked.status.success());
    let inspected: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();
    assert_eq!(backed["receipt"], inspected["receipt"]);
    let output = Command::new(binary)
        .args(["restore-check", "--source"])
        .arg(&package)
        .arg("--destination")
        .arg(&dest)
        .output()
        .unwrap();
    assert!(output.status.success());
    let restored: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(restored["disposition"], "quarantined");
    assert_eq!(restored["reasons"].as_array().unwrap().len(), 3);
    for path in [&package, &dest] {
        let output = Command::new(binary)
            .args(["diagnostics", "--data-root"])
            .arg(path)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
    }
    let output = Command::new(binary)
        .args(["backup-check", "--source"])
        .arg(parent.join("PRIVATE_MISSING_SOURCE"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(!String::from_utf8(output.stderr)
        .unwrap()
        .contains("PRIVATE_MISSING_SOURCE"));
    let output = Command::new(binary)
        .args(["restore-check", "--source"])
        .arg(&package)
        .arg("--force")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}
