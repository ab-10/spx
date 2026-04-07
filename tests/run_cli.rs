//! Integration tests for `spx run`.
//!
//! No network or GCS required: these tests exercise only the local half of
//! the pipeline (user resolution, state persistence, and the rclone
//! availability probe).

use std::process::Command;

/// Minimal project fixture with an existing spx.config.json so `spx run`
/// doesn't try to scaffold from scratch.
fn make_project(dir: &std::path::Path, name: &str) {
    std::fs::write(
        dir.join("spx.config.json"),
        format!("{{\"project_name\":\"{name}\"}}\n"),
    )
    .expect("write spx.config.json");
}

#[test]
fn user_missing_fails_cleanly() {
    let tmp_dir = tempfile::tempdir().expect("tempdir");
    make_project(tmp_dir.path(), "demo");

    let spx_bin = env!("CARGO_BIN_EXE_spx");
    let output = Command::new(spx_bin)
        .args(["run"])
        .current_dir(tmp_dir.path())
        .output()
        .expect("run spx");

    assert!(!output.status.success(), "spx run should have failed");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--user"),
        "stderr should tell the user to pass --user; got:\n{stderr}"
    );
}

#[test]
fn user_flag_persists_to_state() {
    let tmp_dir = tempfile::tempdir().expect("tempdir");
    make_project(tmp_dir.path(), "demo");

    let spx_bin = env!("CARGO_BIN_EXE_spx");
    // Force rclone probe to fail so we exit after resolving/saving the user.
    let output = Command::new(spx_bin)
        .args(["run", "--user", "alice"])
        .current_dir(tmp_dir.path())
        .env("PATH", "/nonexistent")
        .output()
        .expect("run spx");

    // Command is expected to fail because rclone cannot be found.
    assert!(
        !output.status.success(),
        "expected failure due to missing rclone"
    );

    let state_path = tmp_dir.path().join(".spx").join("state.json");
    assert!(
        state_path.exists(),
        ".spx/state.json should have been created"
    );
    let state_text = std::fs::read_to_string(&state_path).expect("read state");
    let state: serde_json::Value = serde_json::from_str(&state_text).expect("parse state");
    assert_eq!(
        state["user"].as_str(),
        Some("alice"),
        "user field should be persisted; got state: {state_text}"
    );
}

#[test]
fn missing_rclone_fails_cleanly() {
    let tmp_dir = tempfile::tempdir().expect("tempdir");
    make_project(tmp_dir.path(), "demo");

    let spx_bin = env!("CARGO_BIN_EXE_spx");
    let output = Command::new(spx_bin)
        .args(["run", "--user", "bob"])
        .current_dir(tmp_dir.path())
        .env("PATH", "/nonexistent")
        .output()
        .expect("run spx");

    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("rclone"),
        "stderr should mention rclone; got:\n{stderr}"
    );
    assert!(
        stderr.contains("brew install rclone") || stderr.contains("rclone.org"),
        "stderr should include install hint; got:\n{stderr}"
    );
}
