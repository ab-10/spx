use anyhow::Result;
use std::env;

use crate::cli::RunArgs;
use crate::commands::api;
use crate::credentials::Credentials;
use crate::ui;

pub fn run(_args: RunArgs, verbose: bool) -> Result<()> {
    let cwd = env::current_dir()?;

    if verbose {
        ui::verbose(&format!("Working directory: {}", cwd.display()));
    }

    let creds = Credentials::require()?;
    let user = &creds.username;

    if verbose {
        ui::verbose(&format!("User: {user}"));
    }

    api::ensure_rclone_available()?;

    let api_url = api::api_url();
    if verbose {
        ui::verbose(&format!("Control plane: {api_url}"));
    }

    ui::step(1, 2, &format!("Syncing project to gs://spx-{user}/app/"));
    api::rclone_sync(&cwd, user, verbose)?;
    ui::success("Sync complete.");

    ui::step(2, 2, "Requesting run on preview environment...");
    let resp = api::post_run(&api_url, &creds.token, verbose)?;

    let url = if resp.provisioning {
        ui::info("First run — provisioning resources. This can take up to 5 minutes.");
        api::poll_until_ready(&api_url, &creds.token, verbose)?
    } else {
        resp.url
    };

    ui::success("Run requested.");
    eprintln!();
    eprintln!("  {}", ui::hyperlink(&url, &url));

    Ok(())
}
