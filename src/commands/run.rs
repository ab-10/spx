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

    let api_url = api::api_url();
    if verbose {
        ui::verbose(&format!("Control plane: {api_url}"));
    }

    ui::step(1, 2, "Packaging project...");
    let archive = api::create_archive(&cwd)?;
    if verbose {
        ui::verbose(&format!("Archive size: {} bytes", archive.len()));
    }
    ui::success("Project packaged.");

    ui::step(2, 2, "Deploying...");
    let resp = api::post_run(&api_url, &creds.token, &archive, verbose)?;

    ui::success("Deployed.");
    eprintln!();
    eprintln!("  {}", ui::hyperlink(&resp.url, &resp.url));

    Ok(())
}
