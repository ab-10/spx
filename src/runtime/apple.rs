use anyhow::{bail, Context, Result};
use std::process::{Command, Stdio};

use super::{ContainerResult, BASE_IMAGE};

pub const MIN_VERSION: &str = "0.10.0";

/// Parse a version string like "0.10.0" or "container 0.10.0" into (major, minor, patch).
fn parse_version(version_output: &str) -> Option<(u32, u32, u32)> {
    // Find the first substring that looks like major.minor.patch
    for word in version_output.split_whitespace() {
        let parts: Vec<&str> = word.split('.').collect();
        if parts.len() == 3 {
            if let (Ok(major), Ok(minor), Ok(patch)) = (
                parts[0].parse::<u32>(),
                parts[1].parse::<u32>(),
                parts[2].parse::<u32>(),
            ) {
                return Some((major, minor, patch));
            }
        }
    }
    None
}

/// Check that a version string meets the minimum required version.
fn check_version(version_output: &str) -> Result<()> {
    let actual = parse_version(version_output).with_context(|| {
        format!("Could not parse Apple Container version from: {version_output}")
    })?;

    let min = parse_version(MIN_VERSION)
        .expect("MIN_VERSION constant is not a valid version");

    if actual < min {
        bail!(
            "Apple Container version {}.{}.{} is too old. \
             Minimum required: {MIN_VERSION}. Update with: brew upgrade container",
            actual.0,
            actual.1,
            actual.2,
        );
    }

    Ok(())
}

/// Ensure the Apple Container CLI is available and meets the minimum version.
/// Unlike Docker, there is no daemon — the CLI spawns VMs on demand.
pub fn ensure_apple_container() -> Result<()> {
    let output = Command::new("container")
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("Apple Container CLI is not installed or not in PATH")?;

    if !output.status.success() {
        bail!("Apple Container CLI is not working. Reinstall with: brew install container");
    }

    let version_output = String::from_utf8_lossy(&output.stdout);
    check_version(version_output.trim())?;

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
        container_ip: ip,
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

    // Apple Container inspect returns: [{"status": "running", ...}]
    let running = json
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|obj| obj.get("status"))
        .and_then(|s| s.as_str())
        .map(|s| s == "running")
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

    // Apple Container inspect returns:
    //   [{"networks": [{"ipv4Address": "192.168.64.6/24", ...}], ...}]
    let addr = json
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|obj| obj.get("networks"))
        .and_then(|nets| nets.as_array())
        .and_then(|nets| nets.first())
        .and_then(|net| net.get("ipv4Address"))
        .and_then(|a| a.as_str())
        .unwrap_or("");

    // Strip CIDR suffix (e.g. "192.168.65.13/24" → "192.168.65.13")
    let ip = addr.split('/').next().unwrap_or("");

    if ip.is_empty() {
        bail!("Could not determine IP address for container '{container_name}'");
    }

    Ok(ip.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_below_minimum() {
        assert!(check_version("0.9.0").is_err());
    }

    #[test]
    fn version_at_minimum() {
        assert!(check_version("0.10.0").is_ok());
    }

    #[test]
    fn version_patch_above() {
        assert!(check_version("0.10.1").is_ok());
    }

    #[test]
    fn version_minor_above() {
        assert!(check_version("0.11.0").is_ok());
    }
}
