use anyhow::{bail, Context, Result};
use std::process::{Command, Stdio};

use super::{ContainerResult, BASE_IMAGE};

/// Ensure Docker daemon is available (5s timeout poll).
pub fn ensure_docker() -> Result<()> {
    use std::thread;
    use std::time::{Duration, Instant};

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

/// Create a Docker container with port mapping.
pub fn create_container(
    project_dir: &str,
    container_name: &str,
    host_port: u16,
) -> Result<ContainerResult> {
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
    Ok(ContainerResult {
        container_id,
        host_port: Some(host_port),
        container_ip: None,
    })
}

/// Check if a port is available by attempting to bind to it.
fn port_is_available(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// Try to create a container, falling back to higher ports if the preferred port is taken.
pub fn create_container_with_fallback(
    project_dir: &str,
    container_name: &str,
) -> Result<ContainerResult> {
    let mut next_port = 3000u16;

    loop {
        let port = (next_port..=40000)
            .find(|p| port_is_available(*p))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Could not find an available port in range 3000–40000. Free a port and try again."
                )
            })?;

        if port != 3000 {
            crate::ui::warn(&format!("Port 3000 is in use, using {port} instead."));
        }

        match create_container(project_dir, container_name, port) {
            Ok(result) => return Ok(result),
            Err(e) => {
                let msg = format!("{e}");
                if msg.contains("port is already allocated")
                    || msg.contains("address already in use")
                {
                    let _ = super::remove_container(super::Runtime::Docker, container_name);
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

/// Check if a Docker container is running using Go template inspect format.
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
