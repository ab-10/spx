pub mod claude;
pub mod link;
pub mod new;
pub mod shell;

use anyhow::Result;

use crate::config::LocalState;
use crate::runtime;
use crate::ui;

pub(crate) fn ensure_container_running(
    container_name: &str,
    state: &mut LocalState,
    cwd: &std::path::Path,
    verbose: bool,
) -> Result<()> {
    if verbose {
        ui::verbose("Checking Apple Container availability...");
    }
    runtime::ensure_available()?;
    if verbose {
        ui::verbose("Apple Container is available.");
    }

    if verbose {
        ui::verbose(&format!(
            "Checking if container '{container_name}' is running..."
        ));
    }
    if runtime::container_is_running(container_name)? {
        if verbose {
            ui::verbose("Container is already running.");
        }
        return Ok(());
    }

    if verbose {
        ui::verbose("Container is not running. Checking if it exists...");
    }
    if runtime::container_exists(container_name)? {
        ui::info("Container exists but is stopped. Starting it...");
        runtime::start_container(container_name)?;
        // Refresh container IP after restart (IPs may change)
        if let Ok(ip) = runtime::get_container_ip(container_name) {
            state.container_ip = Some(ip);
            state.save(cwd)?;
        }
        if verbose {
            ui::verbose("Container started. Launching dev server...");
        }
        let _ = runtime::exec_detached_in_container(
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
    let result = runtime::create_container(project_dir, container_name)?;
    if verbose {
        ui::verbose("Container created.");
    }
    state.container_id = Some(result.container_id);
    state.container_ip = Some(result.container_ip);
    state.save(cwd)?;
    let _ = runtime::exec_detached_in_container(
        container_name,
        &["bash", "-c", "npm run dev"],
    );
    if verbose {
        ui::verbose("Dev server launched.");
    }

    Ok(())
}
