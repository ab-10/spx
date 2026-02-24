use anyhow::{bail, Context, Result};
use std::process::{Command, Stdio};

const BASE_IMAGE: &str = "spawn-base:latest";

/// Ensure Docker is available on the system.
pub fn ensure_docker() -> Result<()> {
    let status = Command::new("docker")
        .arg("info")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("docker is not installed or not in PATH")?;

    if !status.success() {
        bail!("Docker daemon is not running. Start Docker and try again.");
    }
    Ok(())
}

/// Pull the spawn base Docker image.
pub fn pull_base_image() -> Result<()> {
    crate::ui::stream_header(&format!("docker pull {BASE_IMAGE}"));
    let status = Command::new("docker")
        .args(["pull", BASE_IMAGE])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("failed to pull spawn base image")?;

    if !status.success() {
        // Image might be built locally — check if it exists
        let check = Command::new("docker")
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
pub fn build_base_image_if_missing() -> Result<()> {
    let check = Command::new("docker")
        .args(["image", "inspect", BASE_IMAGE])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;

    if check.success() {
        return Ok(());
    }

    crate::ui::info("Building spawn base Docker image...");

    // Write a temporary Dockerfile for the base image
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

    let tmp_dir = std::env::temp_dir().join("spawn-docker-build");
    std::fs::create_dir_all(&tmp_dir)?;
    std::fs::write(tmp_dir.join("Dockerfile"), dockerfile)?;

    crate::ui::stream_header("docker build -t spawn-base:latest .");
    let status = Command::new("docker")
        .args(["build", "-t", BASE_IMAGE, "."])
        .current_dir(&tmp_dir)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("failed to build spawn base image")?;

    // Clean up
    let _ = std::fs::remove_dir_all(&tmp_dir);

    if !status.success() {
        bail!("Failed to build spawn base Docker image.");
    }

    Ok(())
}

/// Create and start a container from the base image, mounting the project directory.
pub fn create_container(project_dir: &str, container_name: &str) -> Result<String> {
    crate::ui::stream_header(&format!(
        "docker run -d --name {container_name} -v {project_dir}:/app -p 3000:3000 {BASE_IMAGE} sleep infinity"
    ));

    let output = Command::new("docker")
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
            BASE_IMAGE,
            "sleep",
            "infinity",
        ])
        .output()
        .context("failed to create container")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to create container: {stderr}");
    }

    let container_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(container_id)
}

/// Execute a command inside a running container, streaming output.
pub fn exec_in_container(container_name: &str, cmd: &[&str]) -> Result<()> {
    let display_cmd = cmd.join(" ");
    crate::ui::stream_header(&format!("docker exec {container_name} {display_cmd}"));

    let status = Command::new("docker")
        .arg("exec")
        .arg(container_name)
        .args(cmd)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("failed to exec '{display_cmd}' in container"))?;

    if !status.success() {
        bail!("Command '{}' failed in container with exit code {:?}", display_cmd, status.code());
    }
    Ok(())
}

/// Execute a command inside a container and capture its stdout.
pub fn exec_in_container_output(container_name: &str, cmd: &[&str]) -> Result<String> {
    let output = Command::new("docker")
        .arg("exec")
        .arg(container_name)
        .args(cmd)
        .output()
        .with_context(|| format!("failed to exec in container: {:?}", cmd))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Command failed in container: {stderr}");
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Execute an interactive command in the container (attaches TTY).
/// If `user` is provided, the command runs as that user (`docker exec -u <user>`).
pub fn exec_interactive(container_name: &str, cmd: &[&str], user: Option<&str>) -> Result<()> {
    let display_cmd = cmd.join(" ");
    let user_flag = user.map(|u| format!(" -u {u}")).unwrap_or_default();
    crate::ui::stream_header(&format!(
        "docker exec -it{user_flag} {container_name} {display_cmd}"
    ));

    let mut docker_cmd = Command::new("docker");
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

/// Check if a container with the given name exists and is running.
pub fn container_is_running(container_name: &str) -> Result<bool> {
    let output = Command::new("docker")
        .args(["inspect", "-f", "{{.State.Running}}", container_name])
        .output()?;

    if !output.status.success() {
        return Ok(false);
    }

    let running = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(running == "true")
}

/// Start a stopped container.
pub fn start_container(container_name: &str) -> Result<()> {
    let status = Command::new("docker")
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
    let output = Command::new("docker")
        .args(["inspect", container_name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    Ok(output.success())
}

/// Drop the user into the running container with an interactive shell.
pub fn attach_shell(container_name: &str) -> Result<()> {
    exec_interactive(container_name, &["bash"], None)
}

/// Stop and remove a container.
pub fn remove_container(container_name: &str) -> Result<()> {
    let _ = Command::new("docker")
        .args(["rm", "-f", container_name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    Ok(())
}
