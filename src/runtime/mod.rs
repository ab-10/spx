pub mod apple;

use anyhow::{bail, Context, Result};
use std::process::{Command, Stdio};

pub const BASE_IMAGE: &str = "spx-base:latest";

const BINARY: &str = "container";

// --- Container creation result ---

pub struct ContainerResult {
    pub container_id: String,
    pub container_ip: String,
}

// --- Public API ---

/// Ensure the container runtime is available.
pub fn ensure_available() -> Result<()> {
    apple::ensure_apple_container()
}

/// Pull the spx base image.
pub fn pull_base_image() -> Result<()> {
    crate::ui::stream_header(&format!("{BINARY} pull {BASE_IMAGE}"));
    let status = Command::new(BINARY)
        .args(["pull", BASE_IMAGE])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("failed to pull spx base image")?;

    if !status.success() {
        let check = Command::new(BINARY)
            .args(["image", "inspect", BASE_IMAGE])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if !check.success() {
            bail!(
                "spx base image '{BASE_IMAGE}' not found. \
                 Build it locally or ensure it is available in a registry."
            );
        }
        crate::ui::warn(&format!(
            "Pull failed but local image '{BASE_IMAGE}' exists — using it."
        ));
    }
    Ok(())
}

/// Build the spx base image from a Dockerfile if it doesn't exist.
pub fn build_base_image_if_missing() -> Result<()> {
    let check = Command::new(BINARY)
        .args(["image", "inspect", BASE_IMAGE])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;

    if check.success() {
        return Ok(());
    }

    crate::ui::info("Building spx base container image...");

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

    let tmp_dir = std::env::temp_dir().join("spx-image-build");
    std::fs::create_dir_all(&tmp_dir)?;
    std::fs::write(tmp_dir.join("Dockerfile"), dockerfile)?;

    crate::ui::stream_header(&format!("{BINARY} build -t spx-base:latest ."));
    let status = Command::new(BINARY)
        .args(["build", "-t", BASE_IMAGE, "."])
        .current_dir(&tmp_dir)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("failed to build spx base image")?;

    let _ = std::fs::remove_dir_all(&tmp_dir);

    if !status.success() {
        bail!("Failed to build spx base container image.");
    }

    Ok(())
}

/// Create a container using Apple Container.
pub fn create_container(project_dir: &str, container_name: &str) -> Result<ContainerResult> {
    apple::create_container(project_dir, container_name)
}

/// Check if a container is running.
pub fn container_is_running(container_name: &str) -> Result<bool> {
    apple::container_is_running(container_name)
}

/// Get the IP address of a container.
pub fn get_container_ip(container_name: &str) -> Result<String> {
    apple::get_container_ip(container_name)
}

// --- Functions that use the container binary ---

/// Execute a command inside a running container, streaming output.
pub fn exec_in_container(container_name: &str, cmd: &[&str]) -> Result<()> {
    let display_cmd = cmd.join(" ");
    crate::ui::stream_header(&format!("{BINARY} exec {container_name} {display_cmd}"));

    let status = Command::new(BINARY)
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
pub fn exec_detached_in_container(container_name: &str, cmd: &[&str]) -> Result<()> {
    let display_cmd = cmd.join(" ");

    let status = Command::new(BINARY)
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
pub fn exec_in_container_as(container_name: &str, cmd: &[&str], user: &str) -> Result<()> {
    let display_cmd = cmd.join(" ");
    crate::ui::stream_header(&format!(
        "{BINARY} exec -u {user} {container_name} {display_cmd}"
    ));

    let status = Command::new(BINARY)
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
    container_name: &str,
    cmd: &[&str],
    user: Option<&str>,
) -> Result<()> {
    let display_cmd = cmd.join(" ");
    let user_flag = user.map(|u| format!(" -u {u}")).unwrap_or_default();
    crate::ui::stream_header(&format!(
        "{BINARY} exec -it{user_flag} {container_name} {display_cmd}"
    ));

    let mut container_cmd = Command::new(BINARY);
    container_cmd.arg("exec").arg("-it");
    if let Some(u) = user {
        container_cmd.arg("-u").arg(u);
    }
    container_cmd.arg(container_name).args(cmd);

    let status = container_cmd
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
pub fn check_in_container_as(container_name: &str, cmd: &[&str], user: &str) -> bool {
    Command::new(BINARY)
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
    container_name: &str,
    cmd: &[&str],
    user: &str,
) -> Result<String> {
    let display_cmd = cmd.join(" ");

    let output = Command::new(BINARY)
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
    container_name: &str,
    cmd: &[&str],
    stdin_data: &str,
) -> Result<()> {
    use std::io::Write;

    let display_cmd = cmd.join(" ");
    crate::ui::stream_header(&format!(
        "{BINARY} exec -i {container_name} {display_cmd}"
    ));

    let mut child = Command::new(BINARY)
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
pub fn start_container(container_name: &str) -> Result<()> {
    let status = Command::new(BINARY)
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
pub fn container_exists(container_name: &str) -> Result<bool> {
    let output = Command::new(BINARY)
        .args(["inspect", container_name])
        .stderr(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Ok(false);
    }
    // Apple Containers returns exit 0 with "[]" for nonexistent containers
    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();
    Ok(trimmed != "[]" && !trimmed.is_empty())
}

/// Stop and remove a container.
pub fn remove_container(container_name: &str) -> Result<()> {
    let _ = Command::new(BINARY)
        .args(["rm", "-f", container_name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_exists_returns_false_for_nonexistent() {
        // Apple Containers `inspect` returns exit 0 with "[]" for
        // nonexistent containers. container_exists must not treat
        // that as the container existing.
        let result = container_exists("spx-nonexistent-test-container-00000").unwrap();
        assert!(!result, "container_exists should return false for a nonexistent container");
    }
}
