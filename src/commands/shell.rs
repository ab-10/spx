use anyhow::Result;
use std::env;

use crate::cli::ShellArgs;
use crate::config::{migrate_if_needed, recover_config, LocalState, SpawnConfig};
use crate::runtime::{self, Runtime};
use crate::ui;

pub fn run(_args: ShellArgs, verbose: bool) -> Result<()> {
    let cwd = env::current_dir()?;

    if verbose {
        ui::verbose(&format!("Working directory: {}", cwd.display()));
    }

    migrate_if_needed(&cwd)?;

    if verbose {
        ui::verbose("Loading spawn config...");
    }
    let config = if SpawnConfig::exists(&cwd) {
        SpawnConfig::load(&cwd)?
    } else {
        recover_config(&cwd)?
    };

    if verbose {
        ui::verbose("Loading local state...");
    }
    let mut state = if LocalState::exists(&cwd) {
        LocalState::load(&cwd)?
    } else {
        let s = LocalState::init(&config.project_name, Runtime::Docker);
        s.save(&cwd)?;
        s
    };

    let container_name = state.container_name.clone();
    let runtime = state.runtime();
    if verbose {
        ui::verbose(&format!(
            "Container: {container_name}, runtime: {runtime}"
        ));
    }

    super::ensure_container_running(runtime, &container_name, &mut state, &cwd, verbose)?;

    ui::info("Opening shell inside the container...");
    if verbose {
        ui::verbose(&format!(
            "Exec: {} exec -it -u claude {container_name} bash",
            runtime.binary()
        ));
    }

    runtime::exec_interactive(runtime, &container_name, &["bash"], Some("claude"))?;

    ui::success("Shell session ended.");

    Ok(())
}
