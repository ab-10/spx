//! Integration test for `spawn init --local --non-interactive`.
//!
//! Runs the real binary against a real Docker daemon and verifies the
//! side-effects: config file, scaffolded project, running container,
//! bind mount, and user setup.
//!
//! Prerequisites: Docker must be running. The spawn-base image will be
//! built automatically if not present.

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
fn init_local_end_to_end() {
    require_docker();

    let project_name = format!("spawn-test-{}", std::process::id());
    let container_name = format!("spawn-{project_name}");
    let _guard = ContainerGuard {
        name: container_name.clone(),
    };

    let tmp_dir = tempfile::tempdir().expect("failed to create temp dir");

    // Run: spawn init --local --non-interactive <project_name>
    let spawn_bin = env!("CARGO_BIN_EXE_spawn");
    let output = Command::new(spawn_bin)
        .args(["init", "--local", "--non-interactive", &project_name])
        .current_dir(tmp_dir.path())
        .output()
        .expect("failed to run spawn binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "spawn init --local failed.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let project_dir = tmp_dir.path().join(&project_name);

    // 1. Config file exists and parses correctly
    let config_path = project_dir.join("spawn.config.json");
    assert!(config_path.exists(), "spawn.config.json not created");

    let config_text = std::fs::read_to_string(&config_path).expect("failed to read config");
    let config: serde_json::Value =
        serde_json::from_str(&config_text).expect("config is not valid JSON");

    assert_eq!(config["project_name"], project_name);
    assert_eq!(config["local_only"], true);
    assert!(
        config["container_name"].is_string(),
        "expected container_name in config"
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

    // 3. Container is running
    let (ok, running_str, _) = run_cmd(
        "docker",
        &["inspect", "-f", "{{.State.Running}}", &container_name],
    );
    assert!(ok, "docker inspect failed — container may not exist");
    assert_eq!(
        running_str, "true",
        "container is not running (got: {running_str})"
    );

    // 4. Bind mount works: write on host, read inside container
    let marker = "spawn-integration-test-marker";
    std::fs::write(project_dir.join(".spawn-test"), marker).expect("failed to write marker file");
    let (ok, cat_out, cat_err) = run_cmd(
        "docker",
        &["exec", &container_name, "cat", "/app/.spawn-test"],
    );
    assert!(
        ok,
        "docker exec cat failed — bind mount may be broken: {cat_err}"
    );
    assert_eq!(
        cat_out, marker,
        "marker file content mismatch inside container"
    );

    // 5. `claude` user works inside the container
    let (ok, whoami_out, whoami_err) = run_cmd(
        "docker",
        &["exec", "-u", "claude", &container_name, "whoami"],
    );
    assert!(ok, "docker exec -u claude whoami failed: {whoami_err}");
    assert_eq!(
        whoami_out, "claude",
        "expected whoami to return 'claude', got: {whoami_out}"
    );
}
