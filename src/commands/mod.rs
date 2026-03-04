pub mod claude;
pub mod link;
pub mod new;
pub mod shell;

use anyhow::Result;

use crate::config::SpawnConfig;
use crate::docker;
use crate::ui;

pub(crate) fn ensure_container_running(container_name: &str, config: &mut SpawnConfig, cwd: &std::path::Path) -> Result<()> {
    docker::ensure_docker()?;

    if docker::container_is_running(container_name)? {
        return Ok(());
    }

    if docker::container_exists(container_name)? {
        ui::info("Container exists but is stopped. Starting it...");
        docker::start_container(container_name)?;
        let _ = docker::exec_in_container(container_name, &["bash", "-c", "npm run dev &"]);
        return Ok(());
    }

    ui::warn("Container not found. Recreating...");
    let project_dir = cwd
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("project path is not valid UTF-8"))?;

    let (_container_id, port) = docker::create_container_with_fallback(project_dir, container_name)?;
    config.port = Some(port);
    config.save(cwd)?;
    let _ = docker::exec_in_container(container_name, &["bash", "-c", "npm run dev &"]);

    Ok(())
}
