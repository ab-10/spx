pub mod apple;
pub mod docker;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::process::{Command, Stdio};

pub const BASE_IMAGE: &str = "spawn-base:latest";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Runtime {
    Docker,
    #[serde(rename = "container")]
    #[value(name = "container")]
    AppleContainer,
}

impl std::fmt::Display for Runtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Runtime::Docker => write!(f, "Docker"),
            Runtime::AppleContainer => write!(f, "Apple Container"),
        }
    }
}

impl Runtime {
    pub fn binary(&self) -> &str {
        match self {
            Runtime::Docker => "docker",
            Runtime::AppleContainer => "container",
        }
    }

    /// Detect the preferred runtime for new containers.
    pub fn detect() -> Result<Self> {
        if Self::is_apple_container_platform() {
            if binary_exists("container") {
                return Ok(Runtime::AppleContainer);
            }
            if binary_exists("docker") {
                bail!(
                    "Apple Container is the default runtime on this system.\n\
                     Install it with: brew install container\n\
                     Or use Docker explicitly: spawn new --runtime docker"
                );
            }
            bail!("No container runtime found. Install Apple Container: brew install container");
        }

        if binary_exists("docker") {
            return Ok(Runtime::Docker);
        }

        bail!("Docker is not installed or not in PATH. Install Docker to use spawn.");
    }

    /// Check if this platform supports Apple Container (macOS 26+ on Apple Silicon).
    fn is_apple_container_platform() -> bool {
        #[cfg(target_os = "macos")]
        {
            is_apple_silicon() && macos_version_major() >= 26
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }
}

fn binary_exists(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Runtime check for Apple Silicon (handles x86_64 Rust toolchain under Rosetta).
#[cfg(target_os = "macos")]
fn is_apple_silicon() -> bool {
    Command::new("uname")
        .arg("-m")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim() == "arm64")
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn macos_version_major() -> u32 {
    Command::new("sw_vers")
        .args(["-productVersion"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| parse_macos_version_major(&s))
        .unwrap_or(0)
}

/// Parse the major version from a `sw_vers -productVersion` string.
///
/// Real-world values: "15.0", "15.6.1", "26.0", "26.1", "26.2", "26.3".
/// Apple jumped from version 15 (Sequoia) to 26 (Tahoe) with year-based versioning.
fn parse_macos_version_major(version: &str) -> u32 {
    version
        .trim()
        .split('.')
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

// --- Container creation result ---

pub struct ContainerResult {
    pub container_id: String,
    pub host_port: Option<u16>,
    pub container_ip: Option<String>,
}

// --- Public API ---
// Functions that need runtime-specific logic dispatch to submodules.
// Functions that just swap the binary name are implemented inline.

/// Ensure the container runtime is available.
pub fn ensure_available(runtime: Runtime) -> Result<()> {
    match runtime {
        Runtime::Docker => docker::ensure_docker(),
        Runtime::AppleContainer => apple::ensure_apple_container(),
    }
}

/// Pull the spawn base image.
pub fn pull_base_image(runtime: Runtime) -> Result<()> {
    let binary = runtime.binary();
    crate::ui::stream_header(&format!("{binary} pull {BASE_IMAGE}"));
    let status = Command::new(binary)
        .args(["pull", BASE_IMAGE])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("failed to pull spawn base image")?;

    if !status.success() {
        let check = Command::new(binary)
            .args(["image", "inspect", BASE_IMAGE])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if !check.success() {
            bail!(
                "spawn base image '{BASE_IMAGE}' not found. \
                 Build it locally or ensure it is available in a registry."
            );
        }
        crate::ui::warn(&format!(
            "Pull failed but local image '{BASE_IMAGE}' exists — using it."
        ));
    }
    Ok(())
}

/// Build the spawn base image from a Dockerfile if it doesn't exist.
pub fn build_base_image_if_missing(runtime: Runtime) -> Result<()> {
    let binary = runtime.binary();
    let check = Command::new(binary)
        .args(["image", "inspect", BASE_IMAGE])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;

    if check.success() {
        return Ok(());
    }

    crate::ui::info(&format!("Building spawn base {runtime} image..."));

    let dockerfile = r#"
FROM node:20-bookworm

# Install system dependencies + GitHub CLI
RUN apt-get update && apt-get install -y \
    git \
    curl \
    sudo \
    && curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg \
       | dd of=/usr/share/keyrings/githubcli-archive-keyring.gpg \
    && echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" \
       | tee /etc/apt/sources.list.d/github-cli.list > /dev/null \
    && apt-get update && apt-get install -y gh \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user (Claude Code requires non-root for --dangerously-skip-permissions)
RUN useradd -m -s /bin/bash claude \
    && usermod -aG sudo claude \
    && echo "claude ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

# Install Claude Code and Vercel CLI globally
RUN npm install -g @anthropic-ai/claude-code vercel

# Install Playwright system deps and browsers
RUN npx playwright install --with-deps chromium

# Set working directory
WORKDIR /app

# Default command
CMD ["bash"]
"#;

    let tmp_dir = std::env::temp_dir().join("spawn-docker-build");
    std::fs::create_dir_all(&tmp_dir)?;
    std::fs::write(tmp_dir.join("Dockerfile"), dockerfile)?;

    crate::ui::stream_header(&format!("{binary} build -t spawn-base:latest ."));
    let status = Command::new(binary)
        .args(["build", "-t", BASE_IMAGE, "."])
        .current_dir(&tmp_dir)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("failed to build spawn base image")?;

    let _ = std::fs::remove_dir_all(&tmp_dir);

    if !status.success() {
        bail!("Failed to build spawn base {runtime} image.");
    }

    Ok(())
}

/// Try to create a container, falling back to higher ports if needed (Docker only).
/// For Apple Container, creates directly (no port mapping needed).
pub fn create_container_with_fallback(
    runtime: Runtime,
    project_dir: &str,
    container_name: &str,
) -> Result<ContainerResult> {
    match runtime {
        Runtime::Docker => docker::create_container_with_fallback(project_dir, container_name),
        Runtime::AppleContainer => apple::create_container(project_dir, container_name),
    }
}

/// Check if a container is running.
pub fn container_is_running(runtime: Runtime, container_name: &str) -> Result<bool> {
    match runtime {
        Runtime::Docker => docker::container_is_running(container_name),
        Runtime::AppleContainer => apple::container_is_running(container_name),
    }
}

/// Get the IP address of a container (Apple Container only, returns None for Docker).
pub fn get_container_ip(runtime: Runtime, container_name: &str) -> Result<Option<String>> {
    match runtime {
        Runtime::AppleContainer => apple::get_container_ip(container_name).map(Some),
        Runtime::Docker => Ok(None),
    }
}

// --- Functions that just swap the binary name ---

/// Execute a command inside a running container, streaming output.
pub fn exec_in_container(runtime: Runtime, container_name: &str, cmd: &[&str]) -> Result<()> {
    let binary = runtime.binary();
    let display_cmd = cmd.join(" ");
    crate::ui::stream_header(&format!("{binary} exec {container_name} {display_cmd}"));

    let status = Command::new(binary)
        .arg("exec")
        .arg(container_name)
        .args(cmd)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("failed to exec '{display_cmd}' in container"))?;

    if !status.success() {
        bail!(
            "Command '{}' failed in container with exit code {:?}",
            display_cmd,
            status.code()
        );
    }
    Ok(())
}

/// Execute a command inside a running container in detached mode (fire-and-forget).
pub fn exec_detached_in_container(
    runtime: Runtime,
    container_name: &str,
    cmd: &[&str],
) -> Result<()> {
    let binary = runtime.binary();
    let display_cmd = cmd.join(" ");

    let status = Command::new(binary)
        .args(["exec", "-d"])
        .arg(container_name)
        .args(cmd)
        .status()
        .with_context(|| format!("failed to exec detached '{display_cmd}' in container"))?;

    if !status.success() {
        bail!(
            "Detached command '{}' failed in container with exit code {:?}",
            display_cmd,
            status.code()
        );
    }
    Ok(())
}

/// Execute a command inside a running container as a specific user, streaming output.
pub fn exec_in_container_as(
    runtime: Runtime,
    container_name: &str,
    cmd: &[&str],
    user: &str,
) -> Result<()> {
    let binary = runtime.binary();
    let display_cmd = cmd.join(" ");
    crate::ui::stream_header(&format!(
        "{binary} exec -u {user} {container_name} {display_cmd}"
    ));

    let status = Command::new(binary)
        .args(["exec", "-u", user])
        .arg(container_name)
        .args(cmd)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("failed to exec '{display_cmd}' as {user} in container"))?;

    if !status.success() {
        bail!(
            "Command '{}' failed in container with exit code {:?}",
            display_cmd,
            status.code()
        );
    }
    Ok(())
}

/// Execute an interactive command in the container (attaches TTY).
pub fn exec_interactive(
    runtime: Runtime,
    container_name: &str,
    cmd: &[&str],
    user: Option<&str>,
) -> Result<()> {
    let binary = runtime.binary();
    let display_cmd = cmd.join(" ");
    let user_flag = user.map(|u| format!(" -u {u}")).unwrap_or_default();
    crate::ui::stream_header(&format!(
        "{binary} exec -it{user_flag} {container_name} {display_cmd}"
    ));

    let mut docker_cmd = Command::new(binary);
    docker_cmd.arg("exec").arg("-it");
    if let Some(u) = user {
        docker_cmd.arg("-u").arg(u);
    }
    docker_cmd.arg(container_name).args(cmd);

    let status = docker_cmd
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("failed to exec interactive '{display_cmd}' in container"))?;

    if !status.success() {
        bail!(
            "Interactive command '{}' exited with code {:?}",
            display_cmd,
            status.code()
        );
    }
    Ok(())
}

/// Run a command silently inside a container as a specific user, returning whether it succeeded.
pub fn check_in_container_as(
    runtime: Runtime,
    container_name: &str,
    cmd: &[&str],
    user: &str,
) -> bool {
    Command::new(runtime.binary())
        .args(["exec", "-u", user])
        .arg(container_name)
        .args(cmd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Execute a command inside a container as a specific user and capture its stdout.
pub fn exec_capture_in_container_as(
    runtime: Runtime,
    container_name: &str,
    cmd: &[&str],
    user: &str,
) -> Result<String> {
    let binary = runtime.binary();
    let display_cmd = cmd.join(" ");

    let output = Command::new(binary)
        .args(["exec", "-u", user])
        .arg(container_name)
        .args(cmd)
        .stderr(Stdio::inherit())
        .output()
        .with_context(|| format!("failed to exec '{display_cmd}' as {user} in container"))?;

    if !output.status.success() {
        bail!(
            "Command '{}' failed in container with exit code {:?}",
            display_cmd,
            output.status.code()
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Execute a command inside a container with data piped to stdin.
pub fn exec_with_stdin_in_container(
    runtime: Runtime,
    container_name: &str,
    cmd: &[&str],
    stdin_data: &str,
) -> Result<()> {
    use std::io::Write;

    let binary = runtime.binary();
    let display_cmd = cmd.join(" ");
    crate::ui::stream_header(&format!(
        "{binary} exec -i {container_name} {display_cmd}"
    ));

    let mut child = Command::new(binary)
        .args(["exec", "-i"])
        .arg(container_name)
        .args(cmd)
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("failed to exec '{display_cmd}' in container"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(stdin_data.as_bytes())
            .with_context(|| format!("failed to write stdin for '{display_cmd}'"))?;
    }

    let status = child
        .wait()
        .with_context(|| format!("failed to wait for '{display_cmd}'"))?;

    if !status.success() {
        bail!(
            "Command '{}' failed in container with exit code {:?}",
            display_cmd,
            status.code()
        );
    }
    Ok(())
}

/// Start a stopped container.
pub fn start_container(runtime: Runtime, container_name: &str) -> Result<()> {
    let status = Command::new(runtime.binary())
        .args(["start", container_name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("failed to start container")?;

    if !status.success() {
        bail!("Failed to start container '{container_name}'.");
    }
    Ok(())
}

/// Check if a container exists at all (running or stopped).
pub fn container_exists(runtime: Runtime, container_name: &str) -> Result<bool> {
    let output = Command::new(runtime.binary())
        .args(["inspect", container_name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    Ok(output.success())
}

/// Stop and remove a container.
pub fn remove_container(runtime: Runtime, container_name: &str) -> Result<()> {
    let _ = Command::new(runtime.binary())
        .args(["rm", "-f", container_name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_macos_version_major ---

    #[test]
    fn parse_sequoia_versions() {
        // macOS 15 Sequoia release versions
        assert_eq!(parse_macos_version_major("15.0"), 15);
        assert_eq!(parse_macos_version_major("15.0.1"), 15);
        assert_eq!(parse_macos_version_major("15.1"), 15);
        assert_eq!(parse_macos_version_major("15.1.1"), 15);
        assert_eq!(parse_macos_version_major("15.2"), 15);
        assert_eq!(parse_macos_version_major("15.3"), 15);
        assert_eq!(parse_macos_version_major("15.3.1"), 15);
        assert_eq!(parse_macos_version_major("15.4"), 15);
        assert_eq!(parse_macos_version_major("15.5"), 15);
        assert_eq!(parse_macos_version_major("15.6"), 15);
        assert_eq!(parse_macos_version_major("15.6.1"), 15);
    }

    #[test]
    fn parse_tahoe_versions() {
        // macOS 26 Tahoe — Apple jumped from 15 to 26 (year-based versioning)
        assert_eq!(parse_macos_version_major("26.0"), 26);
        assert_eq!(parse_macos_version_major("26.1"), 26);
        assert_eq!(parse_macos_version_major("26.2"), 26);
        assert_eq!(parse_macos_version_major("26.3"), 26);
    }

    #[test]
    fn parse_version_with_trailing_newline() {
        // sw_vers output typically has a trailing newline
        assert_eq!(parse_macos_version_major("26.3\n"), 26);
        assert_eq!(parse_macos_version_major("15.6.1\n"), 15);
        assert_eq!(parse_macos_version_major("  26.0  \n"), 26);
    }

    #[test]
    fn parse_version_garbage_returns_zero() {
        assert_eq!(parse_macos_version_major(""), 0);
        assert_eq!(parse_macos_version_major("not-a-version"), 0);
        assert_eq!(parse_macos_version_major(".26"), 0);
    }

    #[test]
    fn parse_future_versions() {
        // Future macOS versions (year-based: 27, 28, ...)
        assert_eq!(parse_macos_version_major("27.0"), 27);
        assert_eq!(parse_macos_version_major("28.1.3"), 28);
    }

    // --- Runtime detection uses uname, not cfg!(target_arch) ---
    //
    // Bug: the original code used #[cfg(target_arch = "aarch64")] to gate
    // Apple Container detection. On an Apple Silicon Mac running an x86_64
    // Rust toolchain (via Rosetta), cfg!(target_arch) is "x86_64" even
    // though the hardware is arm64. This caused detect() to skip Apple
    // Container and fall through to Docker.
    //
    // The fix: use `uname -m` at runtime instead of compile-time cfg.

    #[test]
    #[cfg(target_os = "macos")]
    fn is_apple_silicon_uses_runtime_check() {
        // This test verifies the fix: is_apple_silicon() calls `uname -m`
        // at runtime, so it returns true on arm64 Macs even when the Rust
        // toolchain compiles to x86_64 (Rosetta).
        let uname_output = Command::new("uname")
            .arg("-m")
            .output()
            .expect("uname -m should work");
        let actual_arch = String::from_utf8_lossy(&uname_output.stdout)
            .trim()
            .to_string();

        let result = is_apple_silicon();

        assert_eq!(
            result,
            actual_arch == "arm64",
            "is_apple_silicon() returned {result}, but `uname -m` says '{actual_arch}'. \
             This is the Rosetta bug: ensure we use runtime detection, not cfg!(target_arch)."
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn detect_prefers_apple_container_on_tahoe() {
        // On macOS 26+ Apple Silicon with `container` CLI installed,
        // detect() should return AppleContainer, not Docker.
        if !is_apple_silicon() || macos_version_major() < 26 {
            // Not an Apple Container platform — skip
            return;
        }
        if !binary_exists("container") {
            // container CLI not installed — detect() would bail with
            // an error, not return Docker. That's correct behavior.
            return;
        }

        let runtime = Runtime::detect().expect("detect() should succeed");
        assert_eq!(
            runtime,
            Runtime::AppleContainer,
            "On macOS 26+ Apple Silicon with `container` CLI, detect() must return \
             AppleContainer, not Docker. If this fails, the Rosetta/cfg bug is back."
        );
    }
}
