use anyhow::{bail, Result};
use std::env;
use std::path::Path;

use crate::cli::RunArgs;
use crate::commands::api;
use crate::config::{migrate_if_needed, recover_state, LocalState};
use crate::ui;

pub fn run(args: RunArgs, verbose: bool) -> Result<()> {
    let cwd = env::current_dir()?;

    if verbose {
        ui::verbose(&format!("Working directory: {}", cwd.display()));
    }

    migrate_if_needed(&cwd)?;

    let mut state = if LocalState::exists(&cwd) {
        LocalState::load(&cwd)?
    } else {
        recover_state(&cwd)?
    };

    // Resolve user BEFORE checking rclone so --user persists even if rclone
    // is not installed.
    let user = resolve_user(&args, &mut state, &cwd)?;
    if verbose {
        ui::verbose(&format!("User: {user}"));
    }

    api::ensure_rclone_available()?;

    let api_url = api::api_url();
    if verbose {
        ui::verbose(&format!("Control plane: {api_url}"));
    }

    ui::step(1, 2, &format!("Syncing project to gs://spx-{user}/app/"));
    api::rclone_sync(&cwd, &user, verbose)?;
    ui::success("Sync complete.");

    ui::step(2, 2, "Requesting run on preview environment...");
    let resp = api::post_run(&api_url, &user, verbose)?;

    let url = if resp.provisioning {
        ui::info("First run — provisioning resources. This can take up to 5 minutes.");
        api::poll_until_ready(&api_url, &user, verbose)?
    } else {
        resp.url
    };

    ui::success("Run requested.");
    eprintln!();
    eprintln!("  {}", ui::hyperlink(&url, &url));

    Ok(())
}

fn resolve_user(args: &RunArgs, state: &mut LocalState, cwd: &Path) -> Result<String> {
    if let Some(name) = &args.user {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            bail!("--user cannot be empty");
        }
        state.user = Some(trimmed.to_string());
        state.save(cwd)?;
        return Ok(trimmed.to_string());
    }

    if let Some(name) = &state.user {
        return Ok(name.clone());
    }

    bail!(
        "No user set for this project. Run `spx run --user <name>` to set your identity."
    )
}
