use anyhow::{Context, Result};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::output;

const BASE_IMAGE: &str = "spawn-base:latest";

/// Check that Docker is installed and the daemon is running.
pub async fn check_docker() -> Result<()> {
    which::which("docker")
        .map_err(|_| anyhow::anyhow!("{}", crate::error::SpawnError::DockerNotFound))?;

    let status = Command::new("docker")
        .args(["info"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .context("Failed to run `docker info`")?;

    if !status.success() {
        anyhow::bail!("{}", crate::error::SpawnError::DockerNotRunning);
    }
    Ok(())
}

/// Pull the spawn base Docker image.
pub async fn pull_image() -> Result<()> {
    output::step(1, 1, &format!("Pulling spawn base image ({BASE_IMAGE})..."));

    // For now, check if image exists locally; if not, we'll build a minimal one.
    let check = Command::new("docker")
        .args(["image", "inspect", BASE_IMAGE])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await?;

    if !check.success() {
        output::warn("Spawn base image not found locally. Building it...");
        build_base_image().await?;
    } else {
        output::success("Spawn base image is available.");
    }

    Ok(())
}

/// Build the base Docker image with Node 20, Next.js, and Playwright.
async fn build_base_image() -> Result<()> {
    let dockerfile = r#"FROM node:20-slim

RUN apt-get update && apt-get install -y \
    git \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Install Playwright system dependencies
RUN npx playwright install-deps chromium 2>/dev/null || true
RUN npx playwright install chromium 2>/dev/null || true

WORKDIR /app
"#;

    let tmp_dir = std::env::temp_dir().join("spawn-docker-build");
    std::fs::create_dir_all(&tmp_dir)?;
    std::fs::write(tmp_dir.join("Dockerfile"), dockerfile)?;

    let mut child = Command::new("docker")
        .args(["build", "-t", BASE_IMAGE, "."])
        .current_dir(&tmp_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to start docker build")?;

    // Stream build output
    if let Some(stderr) = child.stderr.take() {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            output::stream_line("docker", &line);
        }
    }

    let status = child.wait().await?;
    if !status.success() {
        anyhow::bail!("Docker build failed");
    }

    // Cleanup
    let _ = std::fs::remove_dir_all(&tmp_dir);
    output::success("Base image built successfully.");
    Ok(())
}

/// Create and start a container for a spawn project.
pub async fn create_container(
    container_name: &str,
    project_dir: &str,
    image: &str,
) -> Result<()> {
    // Remove existing container if any
    let _ = Command::new("docker")
        .args(["rm", "-f", container_name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;

    let status = Command::new("docker")
        .args([
            "run",
            "-d",
            "--name",
            container_name,
            "-v",
            &format!("{project_dir}:/app"),
            "-p",
            "3000:3000",
            "-w",
            "/app",
            image,
            "tail",
            "-f",
            "/dev/null",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .status()
        .await
        .context("Failed to create container")?;

    if !status.success() {
        anyhow::bail!("Failed to create Docker container '{container_name}'");
    }
    Ok(())
}

/// Execute a command inside the container, streaming output.
pub async fn exec_streaming(container_name: &str, cmd: &[&str]) -> Result<bool> {
    let mut args = vec!["exec", container_name];
    args.extend_from_slice(cmd);

    let mut child = Command::new("docker")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to exec in container: {}", cmd.join(" ")))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let stdout_handle = tokio::spawn(async move {
        if let Some(out) = stdout {
            let reader = BufReader::new(out);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                output::stream_line("container", &line);
            }
        }
    });

    let stderr_handle = tokio::spawn(async move {
        if let Some(err) = stderr {
            let reader = BufReader::new(err);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                output::stream_line("container", &line);
            }
        }
    });

    let _ = tokio::join!(stdout_handle, stderr_handle);
    let status = child.wait().await?;
    Ok(status.success())
}

/// Execute a command inside the container and capture output.
pub async fn exec_capture(container_name: &str, cmd: &[&str]) -> Result<String> {
    let mut args = vec!["exec", container_name];
    args.extend_from_slice(cmd);

    let out = Command::new("docker")
        .args(&args)
        .output()
        .await
        .with_context(|| format!("Failed to exec in container: {}", cmd.join(" ")))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!(
            "{}",
            crate::error::SpawnError::CommandFailed {
                command: cmd.join(" "),
                stderr: stderr.to_string(),
            }
        );
    }

    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Execute an interactive command inside the container (e.g., Claude Code).
pub async fn exec_interactive(container_name: &str, cmd: &[&str]) -> Result<()> {
    let mut args = vec!["exec", "-it", container_name];
    args.extend_from_slice(cmd);

    let status = Command::new("docker")
        .args(&args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await
        .with_context(|| format!("Failed to exec interactive command: {}", cmd.join(" ")))?;

    if !status.success() {
        anyhow::bail!("Interactive session exited with non-zero status");
    }
    Ok(())
}

/// Check if a container is running.
pub async fn is_container_running(container_name: &str) -> Result<bool> {
    let out = Command::new("docker")
        .args([
            "inspect",
            "-f",
            "{{.State.Running}}",
            container_name,
        ])
        .output()
        .await?;

    Ok(out.status.success()
        && String::from_utf8_lossy(&out.stdout).trim() == "true")
}

/// Start a stopped container.
pub async fn start_container(container_name: &str) -> Result<()> {
    let status = Command::new("docker")
        .args(["start", container_name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!(
            "{}",
            crate::error::SpawnError::ContainerNotFound(container_name.to_string())
        );
    }
    Ok(())
}

/// Drop into a shell inside the container.
pub async fn shell(container_name: &str) -> Result<()> {
    exec_interactive(container_name, &["bash"]).await
}

/// Get the base image name.
pub fn base_image() -> &'static str {
    BASE_IMAGE
}
