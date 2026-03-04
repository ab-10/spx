//! Integration test: verify the spawn container has a browser executable
//! that `gh auth login` can use to open the OAuth flow.
//!
//! `gh auth login` tries `xdg-open`, `x-www-browser`, `www-browser`, or
//! `wslview`. The spawn-base image (node:20-bookworm) ships none of these,
//! so the OAuth flow fails inside the container.
//!
//! This test replicates the bug by asserting that at least one of those
//! executables exists in the container — it should **fail** until the
//! image is fixed.

use std::process::{Command, Stdio};

/// Run a command and return (success, stdout, stderr).
fn run_cmd(cmd: &str, args: &[&str]) -> (bool, String, String) {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to execute `{cmd}`: {e}"));
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).trim().to_string(),
        String::from_utf8_lossy(&output.stderr).trim().to_string(),
    )
}

/// Fail fast if Docker isn't available.
fn require_docker() {
    let (ok, _, _) = run_cmd("docker", &["info"]);
    assert!(
        ok,
        "Docker daemon is not running. These tests require a running Docker instance."
    );
}

/// RAII guard that removes a Docker container on drop — even on panic.
struct ContainerGuard {
    name: String,
}

impl Drop for ContainerGuard {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .args(["rm", "-f", &self.name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

#[test]
fn container_has_browser_for_gh_auth() {
    require_docker();

    let project_name = format!("spawn-test-ghauth-{}", std::process::id());
    let tmp_dir = tempfile::tempdir().expect("failed to create temp dir");

    // Step 1: Create a real container via `spawn new --non-interactive`
    let spawn_bin = env!("CARGO_BIN_EXE_spawn");
    let output = Command::new(spawn_bin)
        .args(["new", "--non-interactive", &project_name])
        .current_dir(tmp_dir.path())
        .output()
        .expect("failed to run spawn new");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "spawn new failed.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Read container name from .spawn/state.json
    let project_dir = tmp_dir.path().join(&project_name);
    let state_text = std::fs::read_to_string(project_dir.join(".spawn/state.json"))
        .expect("failed to read state");
    let state: serde_json::Value = serde_json::from_str(&state_text).expect("invalid state JSON");
    let container_name = state["container_name"].as_str().unwrap().to_string();
    let _guard = ContainerGuard {
        name: container_name.clone(),
    };

    // Step 2: Check whether any browser executable that `gh auth login`
    // relies on is available inside the container.
    let browser_cmds = ["xdg-open", "x-www-browser", "www-browser", "wslview"];
    let mut found: Vec<&str> = Vec::new();

    for cmd in &browser_cmds {
        let (ok, _, _) = run_cmd(
            "docker",
            &["exec", &container_name, "which", cmd],
        );
        if ok {
            found.push(cmd);
        }
    }

    // Step 3: Assert at least one exists (this FAILS, replicating the bug)
    assert!(
        !found.is_empty(),
        "None of the browser executables that `gh auth login` needs were found \
         in the container.\n\
         Checked: {browser_cmds:?}\n\
         The spawn-base image must install at least one (e.g. `xdg-open` from \
         xdg-utils) so that `gh auth login` can complete the OAuth flow."
    );
}
