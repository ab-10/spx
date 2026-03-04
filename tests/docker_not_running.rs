//! Test that `spawn shell` and `spawn claude` give a clear error when the
//! Docker daemon is not running, instead of the misleading
//! "Container not found. Recreating..." message.
//!
//! Uses a fake `docker` binary (a shell script) that simulates the daemon
//! being down by failing on `docker info`.

use std::os::unix::fs::PermissionsExt;
use std::process::Command;

/// Create a fake `docker` script inside `dir` that always exits 1,
/// simulating a Docker daemon that is not running.
fn create_fake_docker(dir: &std::path::Path) {
    let fake_docker = dir.join("docker");
    std::fs::write(
        &fake_docker,
        "#!/bin/sh\necho 'Cannot connect to the Docker daemon' >&2\nexit 1\n",
    )
    .expect("failed to write fake docker script");
    std::fs::set_permissions(&fake_docker, std::fs::Permissions::from_mode(0o755))
        .expect("failed to chmod fake docker");
}

/// Write spawn.config.json and .spawn/state.json so `spawn shell` / `spawn claude`
/// can load project config and local state.
fn write_config(dir: &std::path::Path) {
    std::fs::write(
        dir.join("spawn.config.json"),
        r#"{"project_name":"test-project"}"#,
    )
    .expect("failed to write config");

    let state_dir = dir.join(".spawn");
    std::fs::create_dir_all(&state_dir).expect("failed to create .spawn dir");
    std::fs::write(
        state_dir.join("state.json"),
        r#"{"container_name":"spawn-test-project","port":3000}"#,
    )
    .expect("failed to write state");
}

#[test]
fn shell_reports_docker_not_running() {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let fake_bin_dir = tempfile::tempdir().expect("failed to create fake bin dir");

    create_fake_docker(fake_bin_dir.path());
    write_config(tmp.path());

    let spawn_bin = env!("CARGO_BIN_EXE_spawn");
    let output = Command::new(spawn_bin)
        .args(["shell"])
        .current_dir(tmp.path())
        // Put the fake docker first in PATH so it shadows the real one.
        .env("PATH", format!("{}:/usr/bin:/bin", fake_bin_dir.path().display()))
        .output()
        .expect("failed to run spawn shell");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stdout}{stderr}");

    assert!(
        !output.status.success(),
        "spawn shell should fail when Docker is not running"
    );

    assert!(
        combined.contains("Docker daemon is not running"),
        "Expected clear 'Docker daemon is not running' error, got:\nstdout: {stdout}\nstderr: {stderr}"
    );

    // Must NOT fall through to the misleading "Container not found. Recreating..." path
    assert!(
        !combined.contains("Container not found"),
        "Should not reach 'Container not found' when Docker daemon is down.\nstdout: {stdout}\nstderr: {stderr}"
    );
}

#[test]
fn claude_reports_docker_not_running() {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let fake_bin_dir = tempfile::tempdir().expect("failed to create fake bin dir");

    create_fake_docker(fake_bin_dir.path());
    write_config(tmp.path());

    let spawn_bin = env!("CARGO_BIN_EXE_spawn");
    let output = Command::new(spawn_bin)
        .args(["claude"])
        .current_dir(tmp.path())
        .env("PATH", format!("{}:/usr/bin:/bin", fake_bin_dir.path().display()))
        .output()
        .expect("failed to run spawn claude");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stdout}{stderr}");

    assert!(
        !output.status.success(),
        "spawn claude should fail when Docker is not running"
    );

    assert!(
        combined.contains("Docker daemon is not running"),
        "Expected clear 'Docker daemon is not running' error, got:\nstdout: {stdout}\nstderr: {stderr}"
    );

    assert!(
        !combined.contains("Container not found"),
        "Should not reach 'Container not found' when Docker daemon is down.\nstdout: {stdout}\nstderr: {stderr}"
    );
}
