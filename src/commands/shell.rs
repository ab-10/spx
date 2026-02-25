use anyhow::Result;
use std::env;

use crate::cli::ShellArgs;
use crate::config::SpawnConfig;
use crate::docker;
use crate::ui;

pub fn run(_args: ShellArgs) -> Result<()> {
    let cwd = env::current_dir()?;
    let mut config = SpawnConfig::load(&cwd)?;
    let container_name = config
        .container_name
        .clone()
        .unwrap_or_else(|| format!("spawn-{}", config.project_name));

    super::ensure_container_running(&container_name, &mut config, &cwd)?;

    ui::info("Opening shell inside the container...");

    docker::exec_interactive(&container_name, &["bash"], Some("claude"))?;

    ui::success("Shell session ended.");

    Ok(())
}
