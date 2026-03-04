use anyhow::Result;
use std::env;

use crate::cli::ShellArgs;
use crate::config::{migrate_if_needed, recover_config, LocalState, SpawnConfig};
use crate::docker;
use crate::ui;

pub fn run(_args: ShellArgs) -> Result<()> {
    let cwd = env::current_dir()?;

    migrate_if_needed(&cwd)?;

    let config = if SpawnConfig::exists(&cwd) {
        SpawnConfig::load(&cwd)?
    } else {
        recover_config(&cwd)?
    };

    let mut state = if LocalState::exists(&cwd) {
        LocalState::load(&cwd)?
    } else {
        let s = LocalState::init(&config.project_name);
        s.save(&cwd)?;
        s
    };

    let container_name = state.container_name.clone();

    super::ensure_container_running(&container_name, &mut state, &cwd)?;

    ui::info("Opening shell inside the container...");

    docker::exec_interactive(&container_name, &["bash"], Some("claude"))?;

    ui::success("Shell session ended.");

    Ok(())
}
