use anyhow::{bail, Context, Result};
use std::process::{Command, Stdio};

use super::{ContainerResult, BASE_IMAGE};

/// Ensure the Apple Container CLI is available.
/// Unlike Docker, there is no daemon — the CLI spawns VMs on demand.
pub fn ensure_apple_container() -> Result<()> {
    let status = Command::new("container")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("Apple Container CLI is not installed or not in PATH")?;

    if !status.success() {
        bail!("Apple Container CLI is not working. Reinstall with: brew install container");
    }
    Ok(())
}

/// Create and start a container using Apple Container.
/// Apple containers get their own IP address — no port mapping needed.
pub fn create_container(project_dir: &str, container_name: &str) -> Result<ContainerResult> {
    crate::ui::stream_header(&format!(
        "container run -d --name {container_name} --volume {project_dir}:/app {BASE_IMAGE} sleep infinity"
    ));

    let output = Command::new("container")
        .args([
            "run",
            "-d",
            "--name",
            container_name,
            "--volume",
            &format!("{project_dir}:/app"),
            "-w",
            "/app",
            BASE_IMAGE,
            "sleep",
            "infinity",
        ])
        .output()
        .context("failed to create Apple container")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to create container: {stderr}");
    }

    let container_id = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // Get the container's IP address
    let ip = get_container_ip(container_name)?;

    Ok(ContainerResult {
        container_id,
        host_port: None,
        container_ip: Some(ip),
    })
}

/// Check if an Apple container is running by inspecting its JSON state.
pub fn container_is_running(container_name: &str) -> Result<bool> {
    let output = Command::new("container")
        .args(["inspect", container_name])
        .output()?;

    if !output.status.success() {
        return Ok(false);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or(serde_json::Value::Null);

    // Apple Container inspect returns JSON — check for running state.
    // The exact field path may vary; try the Docker-compatible path first.
    let running = json
        .get("State")
        .and_then(|s| s.get("Running"))
        .and_then(|r| r.as_bool())
        .unwrap_or(false);

    Ok(running)
}

/// Get the IP address of a running Apple container.
pub fn get_container_ip(container_name: &str) -> Result<String> {
    let output = Command::new("container")
        .args(["inspect", container_name])
        .output()
        .context("failed to inspect container for IP address")?;

    if !output.status.success() {
        bail!("Failed to inspect container '{container_name}' for IP address");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).context("failed to parse container inspect output")?;

    // Try common JSON paths for container IP
    let ip = json
        .get("NetworkSettings")
        .and_then(|ns| ns.get("IPAddress"))
        .and_then(|ip| ip.as_str())
        .or_else(|| {
            json.get("NetworkSettings")
                .and_then(|ns| ns.get("Networks"))
                .and_then(|nets| {
                    nets.as_object()
                        .and_then(|m| m.values().next())
                        .and_then(|net| net.get("IPAddress"))
                        .and_then(|ip| ip.as_str())
                })
        })
        .unwrap_or("")
        .to_string();

    if ip.is_empty() {
        bail!("Could not determine IP address for container '{container_name}'");
    }

    Ok(ip)
}
