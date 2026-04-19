//! Integration tests for `spx run`.
//!
//! No network or GCS required: these tests exercise only the local half of
//! the pipeline (credential loading and the rclone availability probe).

use std::process::Command;

/// Write a fake credentials file so `spx run` gets past the auth check.
fn write_fake_credentials(home: &std::path::Path) {
    let spx_dir = home.join(".spx");
    std::fs::create_dir_all(&spx_dir).expect("create ~/.spx dir");
    std::fs::write(
        spx_dir.join("credentials.json"),
        r#"{"username":"testuser","token":"fake-token"}"#,
    )
    .expect("write credentials.json");
}

#[test]
fn not_logged_in_fails_cleanly() {
    let tmp_dir = tempfile::tempdir().expect("tempdir");

    let spx_bin = env!("CARGO_BIN_EXE_spx");
    let output = Command::new(spx_bin)
        .args(["run"])
        .current_dir(tmp_dir.path())
        .env("HOME", tmp_dir.path())  // no credentials.json here
        .output()
        .expect("run spx");

    assert!(!output.status.success(), "spx run should have failed");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("spx login"),
        "stderr should tell the user to run `spx login`; got:\n{stderr}"
    );
}

#[test]
fn missing_rclone_fails_cleanly() {
    let tmp_dir = tempfile::tempdir().expect("tempdir");
    write_fake_credentials(tmp_dir.path());

    let spx_bin = env!("CARGO_BIN_EXE_spx");
    let output = Command::new(spx_bin)
        .args(["run"])
        .current_dir(tmp_dir.path())
        .env("HOME", tmp_dir.path())
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
