use anyhow::{bail, Context, Result};
use std::process::{Command, Stdio};

const BASE_IMAGE: &str = "spawn-base:latest";

/// Ensure Docker is available on the system.
pub fn ensure_docker() -> Result<()> {
    use std::time::{Duration, Instant};
    use std::thread;

    // Use `docker info -f '{{.ID}}'` — minimal output, still confirms daemon is alive.
    // Spawn + poll with timeout so we don't hang forever if the daemon is unresponsive.
    let mut child = Command::new("docker")
        .args(["info", "-f", "{{.ID}}"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("docker is not installed or not in PATH")?;

    let timeout = Duration::from_secs(5);
    let start = Instant::now();
    loop {
        match child.try_wait()? {
            Some(status) if status.success() => return Ok(()),
            Some(_) => bail!("Docker daemon is not running. Start Docker and try again."),
            None if start.elapsed() >= timeout => {
                let _ = child.kill();
                bail!("Docker daemon did not respond within 5 seconds. Is Docker running?");
            }
            None => thread::sleep(Duration::from_millis(100)),
        }
    }
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
/// The container maps `host_port` to container port 3000.
/// Returns `(container_id, host_port)`.
pub fn create_container(project_dir: &str, container_name: &str, host_port: u16) -> Result<(String, u16)> {
    let port_mapping = format!("{host_port}:3000");
    crate::ui::stream_header(&format!(
        "docker run -d --name {container_name} -v {project_dir}:/app -p {port_mapping} {BASE_IMAGE} sleep infinity"
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
            &port_mapping,
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
    Ok((container_id, host_port))
}

/// Check if a port is available by attempting to bind to it.
fn port_is_available(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// Try to create a container, falling back to higher ports if the preferred port is taken.
/// Starts at port 3000 and increments up to 40000.
/// Returns `(container_id, actual_port)`.
pub fn create_container_with_fallback(project_dir: &str, container_name: &str) -> Result<(String, u16)> {
    let mut next_port = 3000u16;

    loop {
        let port = (next_port..=40000)
            .find(|p| port_is_available(*p))
            .ok_or_else(|| anyhow::anyhow!("Could not find an available port in range 3000–40000. Free a port and try again."))?;

        if port != 3000 {
            crate::ui::warn(&format!("Port 3000 is in use, using {port} instead."));
        }

        match create_container(project_dir, container_name, port) {
            Ok(result) => return Ok(result),
            Err(e) => {
                let msg = format!("{e}");
                if msg.contains("port is already allocated") || msg.contains("address already in use") {
                    let _ = remove_container(container_name);
                    next_port = port + 1;
                    if next_port > 40000 {
                        bail!("Could not find an available port in range 3000–40000.");
                    }
                } else {
                    return Err(e);
                }
            }
        }
    }
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

/// Execute a command inside a running container in detached mode (fire-and-forget).
/// Uses `docker exec -d` so it returns immediately without waiting for the command to finish.
pub fn exec_detached_in_container(container_name: &str, cmd: &[&str]) -> Result<()> {
    let display_cmd = cmd.join(" ");

    let status = Command::new("docker")
        .args(["exec", "-d"])
        .arg(container_name)
        .args(cmd)
        .status()
        .with_context(|| format!("failed to exec detached '{display_cmd}' in container"))?;

    if !status.success() {
        bail!("Detached command '{}' failed in container with exit code {:?}", display_cmd, status.code());
    }
    Ok(())
}

/// Execute a command inside a running container as a specific user, streaming output.
pub fn exec_in_container_as(container_name: &str, cmd: &[&str], user: &str) -> Result<()> {
    let display_cmd = cmd.join(" ");
    crate::ui::stream_header(&format!("docker exec -u {user} {container_name} {display_cmd}"));

    let status = Command::new("docker")
        .args(["exec", "-u", user])
        .arg(container_name)
        .args(cmd)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("failed to exec '{display_cmd}' as {user} in container"))?;

    if !status.success() {
        bail!("Command '{}' failed in container with exit code {:?}", display_cmd, status.code());
    }
    Ok(())
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

/// Run a command silently inside a container as a specific user, returning whether it succeeded.
pub fn check_in_container_as(container_name: &str, cmd: &[&str], user: &str) -> bool {
    Command::new("docker")
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
pub fn exec_capture_in_container_as(container_name: &str, cmd: &[&str], user: &str) -> Result<String> {
    let display_cmd = cmd.join(" ");

    let output = Command::new("docker")
        .args(["exec", "-u", user])
        .arg(container_name)
        .args(cmd)
        .stderr(Stdio::inherit())
        .output()
        .with_context(|| format!("failed to exec '{display_cmd}' as {user} in container"))?;

    if !output.status.success() {
        bail!("Command '{}' failed in container with exit code {:?}", display_cmd, output.status.code());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Execute a command inside a container with data piped to stdin.
pub fn exec_with_stdin_in_container(container_name: &str, cmd: &[&str], stdin_data: &str) -> Result<()> {
    use std::io::Write;

    let display_cmd = cmd.join(" ");
    crate::ui::stream_header(&format!("docker exec -i {container_name} {display_cmd}"));

    let mut child = Command::new("docker")
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

    let status = child.wait().with_context(|| format!("failed to wait for '{display_cmd}'"))?;

    if !status.success() {
        bail!("Command '{}' failed in container with exit code {:?}", display_cmd, status.code());
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

/// Stop and remove a container.
pub fn remove_container(container_name: &str) -> Result<()> {
    let _ = Command::new("docker")
        .args(["rm", "-f", container_name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    Ok(())
}
