//! Integration test for `spawn new --non-interactive`.
//!
//! Runs the real binary against a real Apple Container runtime and verifies the
//! side-effects: config file, scaffolded project, running container,
//! bind mount, and user setup.
//!
//! Prerequisites: Apple Container CLI (`container`) must be available.
//! The spawn-base image will be built automatically if not present.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

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

/// Fail fast if Apple Container CLI isn't available.
fn require_container() {
    let (ok, _, _) = run_cmd("container", &["--version"]);
    assert!(
        ok,
        "Apple Container CLI is not available. These tests require `container` to be installed."
    );
}

/// RAII guard that removes a container on drop — even on panic.
struct ContainerGuard {
    name: String,
}

impl Drop for ContainerGuard {
    fn drop(&mut self) {
        let _ = Command::new("container")
            .args(["rm", "-f", &self.name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

#[test]
fn new_local_end_to_end() {
    require_container();

    let project_name = format!("spawn-test-{}", std::process::id());

    let tmp_dir = tempfile::tempdir().expect("failed to create temp dir");

    // Run: spawn new --non-interactive <project_name>
    let spawn_bin = env!("CARGO_BIN_EXE_spawn");
    let output = Command::new(spawn_bin)
        .args(["new", "--non-interactive", &project_name])
        .current_dir(tmp_dir.path())
        .output()
        .expect("failed to run spawn binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "spawn new failed.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let project_dir = tmp_dir.path().join(&project_name);

    // 1a. spawn.config.json exists and has only project_name
    let config_path = project_dir.join("spawn.config.json");
    assert!(config_path.exists(), "spawn.config.json not created");

    let config_text = std::fs::read_to_string(&config_path).expect("failed to read config");
    let config: serde_json::Value =
        serde_json::from_str(&config_text).expect("config is not valid JSON");

    assert_eq!(config["project_name"], project_name);
    assert!(
        config.get("container_id").is_none(),
        "container_id should not be in spawn.config.json"
    );
    assert!(
        config.get("container_name").is_none(),
        "container_name should not be in spawn.config.json"
    );

    // 1b. .spawn/state.json exists with container_name, container_id, container_ip
    let state_path = project_dir.join(".spawn").join("state.json");
    assert!(state_path.exists(), ".spawn/state.json not created");

    let state_text = std::fs::read_to_string(&state_path).expect("failed to read state");
    let state: serde_json::Value =
        serde_json::from_str(&state_text).expect("state is not valid JSON");

    assert!(
        state["container_name"].is_string(),
        "expected container_name in state"
    );
    let container_name = state["container_name"].as_str().unwrap().to_string();
    assert!(
        container_name.starts_with(&format!("spawn-{project_name}-")),
        "container_name should start with spawn-{{project_name}}-: got {container_name}"
    );
    assert!(
        state["container_id"].is_string(),
        "expected container_id in state"
    );
    assert!(
        state["container_ip"].is_string(),
        "expected container_ip in state"
    );

    // Use the actual container name from state for cleanup
    let _guard = ContainerGuard {
        name: container_name.clone(),
    };

    // 1c. .gitignore contains .spawn/
    let gitignore_path = project_dir.join(".gitignore");
    assert!(gitignore_path.exists(), ".gitignore not found");
    let gitignore = std::fs::read_to_string(&gitignore_path).expect("failed to read .gitignore");
    assert!(
        gitignore.contains(".spawn/"),
        ".gitignore should contain .spawn/"
    );

    // 2. Next.js scaffold exists
    assert!(
        project_dir.join("package.json").exists(),
        "package.json not found — Next.js scaffold failed"
    );
    assert!(
        project_dir.join("src").exists(),
        "src/ directory not found — Next.js scaffold failed"
    );
    // next.config could be .js, .ts, or .mjs
    let has_next_config = project_dir.join("next.config.js").exists()
        || project_dir.join("next.config.ts").exists()
        || project_dir.join("next.config.mjs").exists();
    assert!(has_next_config, "next.config.* not found");

    // 3. Container is running (use Apple Container inspect JSON format)
    let (ok, inspect_out, _) = run_cmd("container", &["inspect", &container_name]);
    assert!(ok, "container inspect failed — container may not exist");
    let inspect: serde_json::Value =
        serde_json::from_str(&inspect_out).expect("inspect output is not valid JSON");
    let status = inspect
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|obj| obj.get("status"))
        .and_then(|s| s.as_str())
        .unwrap_or("unknown");
    assert_eq!(
        status, "running",
        "container is not running (got: {status})"
    );

    // 4. Bind mount works: write on host, read inside container
    let marker = "spawn-integration-test-marker";
    std::fs::write(project_dir.join(".spawn-test"), marker).expect("failed to write marker file");
    let (ok, cat_out, cat_err) = run_cmd(
        "container",
        &["exec", &container_name, "cat", "/app/.spawn-test"],
    );
    assert!(
        ok,
        "container exec cat failed — bind mount may be broken: {cat_err}"
    );
    assert_eq!(
        cat_out, marker,
        "marker file content mismatch inside container"
    );

    // 5. Node.js is available in the container (base image sanity check)
    let (ok, node_out, _) = run_cmd("container", &["exec", &container_name, "node", "--version"]);
    assert!(ok, "node not found in container");
    assert!(
        node_out.starts_with('v'),
        "unexpected node --version output: {node_out}"
    );

    // 6. `spawn claude` precondition: exec as the claude user must work.
    let (ok, whoami_out, whoami_err) = run_cmd(
        "container",
        &["exec", "-u", "claude", &container_name, "whoami"],
    );
    assert!(
        ok,
        "container exec -u claude failed — the claude user does not exist in the container.\n\
         stderr: {whoami_err}"
    );
    assert_eq!(
        whoami_out, "claude",
        "Expected whoami to return 'claude', got: {whoami_out}"
    );
}

/// After `spawn new`, running `spawn claude` must be able to
/// exec into the container as the `claude` user.
#[test]
fn run_claude_after_new() {
    require_container();

    let project_name = format!("spawn-test-run-{}", std::process::id());

    let tmp_dir = tempfile::tempdir().expect("failed to create temp dir");

    // Step 1: Run `spawn new --non-interactive`
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

    // Read the container name from .spawn/state.json
    let project_dir = tmp_dir.path().join(&project_name);
    let state_text = std::fs::read_to_string(project_dir.join(".spawn/state.json"))
        .expect("failed to read state");
    let state: serde_json::Value = serde_json::from_str(&state_text).expect("invalid state JSON");
    let container_name = state["container_name"].as_str().unwrap().to_string();
    let _guard = ContainerGuard {
        name: container_name.clone(),
    };

    // Step 2: Verify the claude user exists in the container.
    let (ok, stdout, stderr) = run_cmd(
        "container",
        &["exec", "-u", "claude", &container_name, "whoami"],
    );
    assert!(
        ok,
        "`container exec -u claude {container_name} whoami` failed.\n\
         stderr: {stderr}"
    );
    assert_eq!(stdout, "claude");

    // Step 3: Verify the claude CLI is available.
    let (ok, which_out, stderr) = run_cmd(
        "container",
        &["exec", "-u", "claude", &container_name, "which", "claude"],
    );
    assert!(
        ok,
        "claude CLI not found in container when running as claude user.\n\
         stderr: {stderr}"
    );
    assert!(
        !which_out.is_empty(),
        "Expected `which claude` to return a path"
    );
}

/// Simulate a partial first run that crashes after scaffolding but before
/// saving config, then verify that a second `spawn new` still succeeds.
#[test]
fn new_retry_after_partial_failure() {
    require_container();

    let project_name = format!("spawn-test-retry-{}", std::process::id());

    let tmp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let project_dir = tmp_dir.path().join(&project_name);

    // --- Simulate a partial first run ---
    std::fs::create_dir_all(&project_dir).expect("failed to create project dir");
    std::fs::write(project_dir.join("package.json"), r#"{"name":"leftover"}"#)
        .expect("failed to write package.json");
    std::fs::create_dir_all(project_dir.join("src")).expect("failed to create src/");
    std::fs::write(
        project_dir.join("next.config.ts"),
        "export default {};",
    )
    .expect("failed to write next.config.ts");

    assert!(
        !project_dir.join("spawn.config.json").exists(),
        "setup error: config should not exist yet"
    );

    // --- Run spawn new (the retry) ---
    let spawn_bin = env!("CARGO_BIN_EXE_spawn");
    let output = Command::new(spawn_bin)
        .args(["new", "--non-interactive", &project_name])
        .current_dir(tmp_dir.path())
        .output()
        .expect("failed to run spawn binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Read container name from state for cleanup
    let state_path = project_dir.join(".spawn/state.json");
    if state_path.exists() {
        let state_text = std::fs::read_to_string(&state_path).unwrap_or_default();
        if let Ok(state) = serde_json::from_str::<serde_json::Value>(&state_text) {
            if let Some(name) = state["container_name"].as_str() {
                let _ = Command::new("container")
                    .args(["rm", "-f", name])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
        }
    }

    assert!(
        output.status.success(),
        "spawn new should recover from a partial previous run, \
         but it failed.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    assert!(
        project_dir.join("spawn.config.json").exists(),
        "spawn.config.json not created after retry"
    );
    assert!(
        project_dir.join("package.json").exists(),
        "package.json missing after retry"
    );
}

/// Verify that `spawn new` exits on its own and does NOT attach to the
/// container shell.
#[test]
fn new_does_not_attach_to_container() {
    require_container();

    let project_name = format!("spawn-test-noattach-{}", std::process::id());

    let tmp_dir = tempfile::tempdir().expect("failed to create temp dir");

    let spawn_bin = env!("CARGO_BIN_EXE_spawn");
    let start = Instant::now();
    let child = Command::new(spawn_bin)
        .args(["new", &project_name])
        .current_dir(tmp_dir.path())
        // Pipe stdin so attach_shell can't read from our terminal
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn process");

    let output = child
        .wait_with_output()
        .expect("failed to wait for spawn process");
    let elapsed = start.elapsed();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "spawn new failed.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // The process should exit promptly after setup — not hang on attach_shell.
    assert!(
        elapsed < Duration::from_secs(300),
        "process took {elapsed:?} — likely hung on attach_shell"
    );

    // stdout should NOT contain the "Dropping you into the container" message
    assert!(
        !stdout.contains("Dropping you into the container"),
        "spawn new should not drop into the container shell.\nstdout:\n{stdout}"
    );

    // Config should still be written correctly
    let project_dir = tmp_dir.path().join(&project_name);
    assert!(
        project_dir.join("spawn.config.json").exists(),
        "spawn.config.json not created"
    );

    // Clean up container using name from state
    let state_path = project_dir.join(".spawn/state.json");
    if state_path.exists() {
        let state_text = std::fs::read_to_string(&state_path).unwrap_or_default();
        if let Ok(state) = serde_json::from_str::<serde_json::Value>(&state_text) {
            if let Some(name) = state["container_name"].as_str() {
                let _ = Command::new("container")
                    .args(["rm", "-f", name])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
        }
    }
}
