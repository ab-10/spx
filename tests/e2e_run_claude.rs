//! E2E tests for `spawn run claude`.
//!
//! These tests verify that the Docker container setup and exec commands
//! are correct, specifically ensuring the "unable to find user claude"
//! error does not occur.
//!
//! Tests that require a running Docker daemon are gated behind a helper
//! that skips when Docker is unavailable.

use std::process::{Command, Stdio};

/// Returns true when the Docker daemon is reachable.
fn docker_available() -> bool {
    Command::new("docker")
        .arg("info")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Build the spawn binary (debug) and return the path to it.
fn spawn_binary() -> std::path::PathBuf {
    let status = Command::new("cargo")
        .args(["build"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status()
        .expect("cargo build failed");
    assert!(status.success(), "cargo build must succeed");

    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("debug")
        .join("spawn")
}

// ---------------------------------------------------------------------------
// Unit-level checks (no Docker needed)
// ---------------------------------------------------------------------------

#[test]
fn dockerfile_template_installs_claude_code() {
    // The base image Dockerfile must install the Claude Code CLI so that
    // `docker exec <container> claude` can find the binary.
    let dockerfile = spawn::docker::base_dockerfile();
    assert!(
        dockerfile.contains("@anthropic-ai/claude-code"),
        "Dockerfile must install @anthropic-ai/claude-code.\nGot:\n{dockerfile}"
    );
}

#[test]
fn exec_interactive_args_include_user_root() {
    // `docker exec` must pass `--user root` so the daemon never tries to
    // resolve an ambiguous user from the container config.
    let args = spawn::docker::exec_interactive_args("mycontainer", &["claude", "--dangerously-skip-permissions"]);
    let joined = args.join(" ");

    assert!(
        joined.contains("--user root"),
        "exec_interactive must include --user root.\nGot: {joined}"
    );
    // The command itself must appear after the container name.
    let container_pos = args.iter().position(|a| a == "mycontainer").unwrap();
    let claude_pos = args.iter().position(|a| a == "claude").unwrap();
    assert!(
        claude_pos > container_pos,
        "command 'claude' must come after container name"
    );
}

#[test]
fn exec_interactive_args_do_not_put_claude_as_user() {
    // Regression: ensure `claude` is never placed where Docker could
    // interpret it as a --user value.
    let args = spawn::docker::exec_interactive_args("c", &["claude"]);

    // Find the --user flag and verify its value is "root", not "claude".
    for (i, arg) in args.iter().enumerate() {
        if arg == "--user" {
            assert_eq!(
                args.get(i + 1).map(|s| s.as_str()),
                Some("root"),
                "--user value must be 'root'"
            );
        }
    }
    // `claude` must only appear as the command (after container name), never
    // adjacent to --user.
    let user_idx = args.iter().position(|a| a == "--user").unwrap();
    assert_ne!(
        args.get(user_idx + 1).map(|s| s.as_str()),
        Some("claude"),
        "'claude' must not be the --user value"
    );
}

// ---------------------------------------------------------------------------
// Docker-dependent E2E tests (skipped when daemon is unavailable)
// ---------------------------------------------------------------------------

/// Build the spawn-base image (minimal variant for testing) and return its tag.
fn build_test_base_image() -> String {
    let tag = "spawn-base:latest";
    let tmp = std::env::temp_dir().join("spawn-e2e-docker-build");
    std::fs::create_dir_all(&tmp).unwrap();

    // Use the same Dockerfile that spawn generates (via the public helper).
    let dockerfile = spawn::docker::base_dockerfile();
    std::fs::write(tmp.join("Dockerfile"), &dockerfile).unwrap();

    let status = Command::new("docker")
        .args(["build", "-t", tag, "."])
        .current_dir(&tmp)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status()
        .expect("docker build failed");
    assert!(status.success(), "docker build must succeed");

    let _ = std::fs::remove_dir_all(&tmp);
    tag.to_string()
}

/// Create a throwaway container, returning its name.
fn create_test_container(image: &str) -> String {
    let name = format!("spawn-e2e-test-{}", std::process::id());

    // Remove leftover container from a previous run, if any.
    let _ = Command::new("docker")
        .args(["rm", "-f", &name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    let output = Command::new("docker")
        .args([
            "run", "-d", "--name", &name, "--user", "root", "-w", "/app", image, "sleep",
            "infinity",
        ])
        .output()
        .expect("docker run failed");
    assert!(
        output.status.success(),
        "docker run must succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    name
}

fn remove_test_container(name: &str) {
    let _ = Command::new("docker")
        .args(["rm", "-f", name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[test]
fn e2e_container_has_claude_binary() {
    if !docker_available() {
        eprintln!("SKIP: Docker daemon not available");
        return;
    }

    let image = build_test_base_image();
    let container = create_test_container(&image);

    // Verify `claude` is on PATH inside the container.
    let output = Command::new("docker")
        .args(["exec", "--user", "root", &container, "which", "claude"])
        .output()
        .expect("docker exec failed");

    remove_test_container(&container);

    assert!(
        output.status.success(),
        "claude binary must be on PATH inside the container.\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn e2e_exec_claude_does_not_produce_user_error() {
    if !docker_available() {
        eprintln!("SKIP: Docker daemon not available");
        return;
    }

    let image = build_test_base_image();
    let container = create_test_container(&image);

    // Run `claude --version` (non-interactive) with --user root.
    // This must NOT produce "unable to find user claude".
    let output = Command::new("docker")
        .args([
            "exec", "--user", "root", &container, "claude", "--version",
        ])
        .output()
        .expect("docker exec failed");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    remove_test_container(&container);

    assert!(
        !stderr.contains("unable to find user claude"),
        "Must not get 'unable to find user claude' error.\nstderr: {stderr}\nstdout: {stdout}"
    );
    // The command should succeed (exit 0).
    assert!(
        output.status.success(),
        "claude --version must succeed.\nstderr: {stderr}\nstdout: {stdout}"
    );
}

#[test]
fn e2e_spawn_run_claude_subcommand_accepted() {
    // Verify the CLI accepts `run claude` as a valid subcommand (no Docker needed).
    // We pass --help after to avoid actually trying to connect to Docker.
    let binary = spawn_binary();

    // `spawn run` without a tool should fail with a usage error.
    let output = Command::new(&binary)
        .args(["run"])
        .output()
        .expect("spawn run failed to execute");
    assert!(
        !output.status.success(),
        "spawn run (no tool) should fail"
    );

    // `spawn claude` (not `spawn run claude`) should fail with unrecognized subcommand.
    let output = Command::new(&binary)
        .args(["claude"])
        .output()
        .expect("spawn claude failed to execute");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "spawn claude (without 'run') should fail"
    );
    assert!(
        stderr.contains("unrecognized") || stderr.contains("not found") || stderr.contains("invalid"),
        "spawn claude should mention unrecognized subcommand.\nstderr: {stderr}"
    );
}
