pub mod claude;
pub mod link;
pub mod new;
pub mod shell;

use anyhow::Result;

use crate::config::LocalState;
use crate::runtime::{self, Runtime};
use crate::ui;

pub(crate) fn ensure_container_running(
    runtime: Runtime,
    container_name: &str,
    state: &mut LocalState,
    cwd: &std::path::Path,
    verbose: bool,
) -> Result<()> {
    if verbose {
        ui::verbose(&format!("Checking {} availability...", runtime));
    }
    runtime::ensure_available(runtime)?;
    if verbose {
        ui::verbose(&format!("{} is available.", runtime));
    }

    if verbose {
        ui::verbose(&format!(
            "Checking if container '{container_name}' is running..."
        ));
    }
    if runtime::container_is_running(runtime, container_name)? {
        if verbose {
            ui::verbose("Container is already running.");
        }
        return Ok(());
    }

    if verbose {
        ui::verbose("Container is not running. Checking if it exists...");
    }
    if runtime::container_exists(runtime, container_name)? {
        ui::info("Container exists but is stopped. Starting it...");
        runtime::start_container(runtime, container_name)?;
        // Refresh container IP after restart (Apple Container IPs may change)
        if let Ok(Some(ip)) = runtime::get_container_ip(runtime, container_name) {
            state.container_ip = Some(ip);
            state.save(cwd)?;
        }
        if verbose {
            ui::verbose("Container started. Launching dev server...");
        }
        let _ = runtime::exec_detached_in_container(
            runtime,
            container_name,
            &["bash", "-c", "npm run dev"],
        );
        if verbose {
            ui::verbose("Dev server launched.");
        }
        return Ok(());
    }

    ui::warn("Container not found. Recreating...");
    let project_dir = cwd
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("project path is not valid UTF-8"))?;

    if verbose {
        ui::verbose(&format!(
            "Creating container for '{project_dir}'..."
        ));
    }
    let result = runtime::create_container_with_fallback(runtime, project_dir, container_name)?;
    if verbose {
        if let Some(port) = result.host_port {
            ui::verbose(&format!("Container created on port {port}."));
        } else {
            ui::verbose("Container created.");
        }
    }
    state.container_id = Some(result.container_id);
    state.port = result.host_port;
    state.container_ip = result.container_ip;
    state.save(cwd)?;
    let _ = runtime::exec_detached_in_container(
        runtime,
        container_name,
        &["bash", "-c", "npm run dev"],
    );
    if verbose {
        ui::verbose("Dev server launched.");
    }

    Ok(())
}
