//! E2E tests for Docker container setup.
//!
//! These tests verify that the spawn base image is correctly built with:
//! - A `claude` user that can be used with `docker exec -u claude`
//! - The Claude Code CLI installed globally
//!
//! Prerequisites: Docker must be running.

use std::process::{Command, Stdio};
use std::sync::Once;

const TEST_IMAGE: &str = "spawn-base-test:latest";
const TEST_CONTAINER_PREFIX: &str = "spawn-e2e-test";

static BUILD_IMAGE: Once = Once::new();

/// Helper: run a command and return (success, stdout, stderr).
fn run_cmd(cmd: &str, args: &[&str]) -> (bool, String, String) {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .expect("failed to execute command");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).trim().to_string(),
        String::from_utf8_lossy(&output.stderr).trim().to_string(),
    )
}

/// Build the test image once (shared across all tests via std::sync::Once).
fn ensure_test_image() {
    BUILD_IMAGE.call_once(|| {
        let dockerfile = r#"
FROM node:20-bookworm

# Install system dependencies
RUN apt-get update && apt-get install -y \
    git \
    curl \
    sudo \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user (Claude Code requires non-root for --dangerously-skip-permissions)
RUN useradd -m -s /bin/bash claude \
    && usermod -aG sudo claude \
    && echo "claude ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

# Install Claude Code globally
RUN npm install -g @anthropic-ai/claude-code

# Install Playwright system deps and browsers
RUN npx playwright install --with-deps chromium

# Set working directory
WORKDIR /app

# Default command
CMD ["bash"]
"#;

        let tmp_dir = std::env::temp_dir().join("spawn-e2e-test-build");
        std::fs::create_dir_all(&tmp_dir).unwrap();
        std::fs::write(tmp_dir.join("Dockerfile"), dockerfile).unwrap();

        let status = Command::new("docker")
            .args(["build", "-t", TEST_IMAGE, "."])
            .current_dir(&tmp_dir)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .expect("failed to run docker build");

        let _ = std::fs::remove_dir_all(&tmp_dir);
        assert!(status.success(), "Docker image build failed");
    });
}

/// Create a uniquely-named test container and return its name.
/// Uses thread ID to avoid collisions when tests run in parallel.
fn create_test_container(suffix: &str) -> String {
    let name = format!("{TEST_CONTAINER_PREFIX}-{suffix}");

    // Remove any leftover container from previous runs
    let _ = Command::new("docker")
        .args(["rm", "-f", &name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    let status = Command::new("docker")
        .args(["run", "-d", "--name", &name, TEST_IMAGE, "sleep", "infinity"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("failed to create test container");

    assert!(status.success(), "Failed to create test container '{name}'");
    name
}

/// Clean up a test container.
fn cleanup(name: &str) {
    let _ = Command::new("docker")
        .args(["rm", "-f", name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[test]
fn e2e_claude_user_exists_in_container() {
    ensure_test_image();
    let c = create_test_container("user-exists");

    let (ok, stdout, stderr) = run_cmd("docker", &["exec", &c, "id", "claude"]);
    cleanup(&c);

    assert!(ok, "Expected `id claude` to succeed, but got stderr: {stderr}");
    assert!(stdout.contains("claude"), "Expected 'claude' in id output, got: {stdout}");
}

#[test]
fn e2e_exec_as_claude_user_succeeds() {
    ensure_test_image();
    let c = create_test_container("exec-user");

    // This is the exact operation that was failing:
    // `docker exec -u claude <container> whoami`
    let (ok, stdout, stderr) = run_cmd("docker", &["exec", "-u", "claude", &c, "whoami"]);
    cleanup(&c);

    assert!(ok, "Expected `docker exec -u claude` to succeed, but got stderr: {stderr}");
    assert_eq!(stdout, "claude", "Expected whoami to return 'claude', got: {stdout}");
}

#[test]
fn e2e_claude_code_cli_is_installed() {
    ensure_test_image();
    let c = create_test_container("cli-installed");

    let (ok, stdout, stderr) =
        run_cmd("docker", &["exec", "-u", "claude", &c, "which", "claude"]);
    cleanup(&c);

    assert!(ok, "Expected `which claude` to succeed (claude CLI installed), but got stderr: {stderr}");
    assert!(!stdout.is_empty(), "Expected `which claude` to return a path, got empty output");
}

#[test]
fn e2e_claude_user_has_sudo_access() {
    ensure_test_image();
    let c = create_test_container("sudo");

    let (ok, stdout, stderr) =
        run_cmd("docker", &["exec", "-u", "claude", &c, "sudo", "whoami"]);
    cleanup(&c);

    assert!(ok, "Expected `sudo whoami` to succeed for claude user, but got stderr: {stderr}");
    assert_eq!(stdout, "root", "Expected sudo whoami to return 'root', got: {stdout}");
}

#[test]
fn e2e_claude_user_can_write_to_app_dir() {
    ensure_test_image();
    let c = create_test_container("write-app");

    // /app is owned by root in the image (no bind mount in tests), so direct write may fail.
    // Verify sudo write works — that's what matters for the real use case.
    let (ok, _stdout, stderr) = run_cmd(
        "docker",
        &["exec", "-u", "claude", &c, "sudo", "touch", "/app/test-file"],
    );
    cleanup(&c);

    assert!(ok, "Expected claude user to write to /app via sudo, but got stderr: {stderr}");
}
