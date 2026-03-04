pub mod claude;
pub mod link;
pub mod new;
pub mod shell;

use anyhow::Result;

use crate::config::LocalState;
use crate::docker;
use crate::ui;

pub(crate) fn ensure_container_running(container_name: &str, state: &mut LocalState, cwd: &std::path::Path, verbose: bool) -> Result<()> {
    if verbose { ui::verbose("Checking Docker availability..."); }
    docker::ensure_docker()?;
    if verbose { ui::verbose("Docker is available."); }

    if verbose { ui::verbose(&format!("Checking if container '{container_name}' is running...")); }
    if docker::container_is_running(container_name)? {
        if verbose { ui::verbose("Container is already running."); }
        return Ok(());
    }

    if verbose { ui::verbose("Container is not running. Checking if it exists..."); }
    if docker::container_exists(container_name)? {
        ui::info("Container exists but is stopped. Starting it...");
        docker::start_container(container_name)?;
        if verbose { ui::verbose("Container started. Launching dev server..."); }
        let _ = docker::exec_detached_in_container(container_name, &["bash", "-c", "npm run dev"]);
        if verbose { ui::verbose("Dev server launched."); }
        return Ok(());
    }

    ui::warn("Container not found. Recreating...");
    let project_dir = cwd
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("project path is not valid UTF-8"))?;

    if verbose { ui::verbose(&format!("Creating container with fallback ports for '{project_dir}'...")); }
    let (_container_id, port) = docker::create_container_with_fallback(project_dir, container_name)?;
    if verbose { ui::verbose(&format!("Container created on port {port}.")); }
    state.port = Some(port);
    state.save(cwd)?;
    let _ = docker::exec_detached_in_container(container_name, &["bash", "-c", "npm run dev"]);
    if verbose { ui::verbose("Dev server launched."); }

    Ok(())
}
