pub mod github;
pub mod stackauth;
pub mod vercel;

use anyhow::Result;
use dialoguer::Confirm;

use crate::config::SpawnConfig;
use crate::output;

/// Prompt the user to connect cloud wiring if not already connected.
/// Returns true if the user chose to connect (or was already connected).
pub async fn ensure_cloud_connected(
    config: &mut SpawnConfig,
    project_dir: &std::path::Path,
) -> Result<bool> {
    if config.is_cloud_connected() {
        return Ok(true);
    }

    output::warn("This project isn't connected to the cloud yet.");
    let connect = Confirm::new()
        .with_prompt("Connect now?")
        .default(true)
        .interact()?;

    if !connect {
        return Ok(false);
    }

    wire_cloud(config, project_dir).await?;
    Ok(true)
}

/// Perform full cloud wiring: Vercel Postgres, Stack Auth, GitHub, Vercel project.
pub async fn wire_cloud(config: &mut SpawnConfig, project_dir: &std::path::Path) -> Result<()> {
    output::header("Connecting cloud services");

    // 1. Verify CLIs are available and authenticated
    vercel::check_cli().await?;
    github::check_cli().await?;

    // 2. Provision Vercel project and Postgres
    output::step(1, 4, "Provisioning Vercel project and Postgres database...");
    let vercel_config = vercel::provision_project(&config.name, project_dir).await?;
    config.vercel = Some(vercel_config);

    // 3. Set up Stack Auth
    output::step(2, 4, "Configuring Stack Auth...");
    let stack_config =
        stackauth::setup(&config.container_name, project_dir).await?;
    config.stack_auth = Some(stack_config);

    // 4. Create GitHub repo and push
    output::step(3, 4, "Creating GitHub repository...");
    let gh_config = github::create_repo(&config.name, project_dir).await?;
    config.github = Some(gh_config);

    // 5. Sync env vars to Vercel
    output::step(4, 4, "Syncing environment variables to Vercel...");
    vercel::sync_env_vars(project_dir).await?;

    config.cloud_connected = true;
    config.save(project_dir)?;

    output::success("Cloud wiring complete.");
    Ok(())
}
