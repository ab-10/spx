use anyhow::Result;
use std::env;

use crate::cli::ShellArgs;
use crate::config::{migrate_if_needed, recover_config, LocalState, SpxConfig};
use crate::runtime;
use crate::ui;

pub fn run(_args: ShellArgs, verbose: bool) -> Result<()> {
    let cwd = env::current_dir()?;

    if verbose {
        ui::verbose(&format!("Working directory: {}", cwd.display()));
    }

    migrate_if_needed(&cwd)?;

    if verbose {
        ui::verbose("Loading spx config...");
    }
    let config = if SpxConfig::exists(&cwd) {
        SpxConfig::load(&cwd)?
    } else {
        recover_config(&cwd)?
    };

    if verbose {
        ui::verbose("Loading local state...");
    }
    let mut state = if LocalState::exists(&cwd) {
        LocalState::load(&cwd)?
    } else {
        let s = LocalState::init(&config.project_name);
        s.save(&cwd)?;
        s
    };

    let container_name = state.container_name.clone();
    if verbose {
        ui::verbose(&format!("Container: {container_name}"));
    }

    super::ensure_container_running(&container_name, &mut state, &cwd, verbose)?;

    ui::info("Opening shell inside the container...");
    if verbose {
        ui::verbose(&format!(
            "Exec: container exec -it -u claude {container_name} bash"
        ));
    }

    runtime::exec_interactive(&container_name, &["bash"], Some("claude"))?;

    ui::success("Shell session ended.");

    Ok(())
}
