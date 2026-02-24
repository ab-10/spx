use anyhow::Result;
use std::env;

use crate::cli::DeployArgs;
use crate::config::SpawnConfig;
use crate::docker;
use crate::ui;

pub fn run(args: DeployArgs) -> Result<()> {
    let cwd = env::current_dir()?;
    let mut config = SpawnConfig::load(&cwd)?;

    // If local-only, prompt for cloud wiring first
    if !config.is_cloud_connected() {
        prompt_cloud_connect(&cwd, &mut config)?;
    }

    let fallback_name = format!("spawn-{}", config.project_name);
    let container_name = config
        .container_name
        .as_deref()
        .unwrap_or(&fallback_name);

    let total = if args.force { 2 } else { 3 };
    let mut step = 1;

    // Step 1 (unless --force): Run tests
    if !args.force {
        ui::step(step, total, "Running Playwright test suite...");
        let test_result = docker::exec_in_container(container_name, &["npm", "test"]);
        if let Err(e) = test_result {
            ui::error(&format!("Tests failed: {e}"));
            ui::error("Deploy blocked. Fix tests or use `spawn deploy --force` to skip.");
            anyhow::bail!("Test suite failed — deploy aborted.");
        }
        ui::success("All tests passed.");
        step += 1;
    } else {
        ui::warn("Skipping test gate (--force).");
    }

    // Step: Push to main
    ui::step(step, total, "Pushing to main...");
    docker::exec_in_container(container_name, &["git", "add", "-A"])?;
    let _ = docker::exec_in_container(
        container_name,
        &["git", "commit", "-m", "Deploy via spawn deploy"],
    );
    docker::exec_in_container(
        container_name,
        &["git", "push", "origin", "main"],
    )?;
    step += 1;

    // Step: Vercel auto-deploys via GitHub integration, print URL
    ui::step(step, total, "Vercel auto-deploying via GitHub integration...");

    let production_url = get_production_url(container_name, &config);

    ui::success("Deploy triggered.");
    if let Some(url) = &production_url {
        ui::info(&format!(
            "Production URL: {}",
            ui::hyperlink(url, url)
        ));
    } else {
        ui::info("Vercel will deploy automatically from the main branch.");
    }

    ui::next_step("Visit the production URL to verify the deploy.");

    Ok(())
}

/// Prompt for cloud connection (identical flow as preview).
fn prompt_cloud_connect(cwd: &std::path::Path, config: &mut SpawnConfig) -> Result<()> {
    ui::warn("This project isn't connected to the cloud yet. Connect now? [Y/n]");

    let confirm = dialoguer::Confirm::new()
        .with_prompt("Connect now?")
        .default(true)
        .interact()?;

    if !confirm {
        anyhow::bail!("Cloud connection required for deployment.");
    }

    let fallback_name = format!("spawn-{}", config.project_name);
    let container_name = config
        .container_name
        .as_deref()
        .unwrap_or(&fallback_name);
    let project_name = &config.project_name;

    ui::info("Provisioning cloud services...");

    // Provision Vercel Postgres
    ui::step(1, 4, "Provisioning Vercel Postgres...");
    docker::exec_in_container(container_name, &["npx", "vercel", "link", "--yes"])?;
    let store_name = format!("{project_name}-db");
    docker::exec_in_container(
        container_name,
        &["npx", "vercel", "stores", "create", "postgres", &store_name, "--yes"],
    )?;
    docker::exec_in_container(container_name, &["npx", "vercel", "env", "pull", ".env.local"])?;

    // Stack Auth
    ui::step(2, 4, "Configuring Stack Auth...");
    let _ = docker::exec_in_container(
        container_name,
        &["npx", "@stackframe/init-stack", "--no-browser"],
    );

    // Create GitHub repo
    ui::step(3, 4, "Creating GitHub repo...");
    let _ = docker::exec_in_container(container_name, &["git", "init"]);
    docker::exec_in_container(container_name, &["git", "add", "-A"])?;
    let _ = docker::exec_in_container(
        container_name,
        &["git", "commit", "-m", "Initial commit via spawn"],
    );
    docker::exec_in_container(
        container_name,
        &["gh", "repo", "create", project_name, "--private", "--source", ".", "--push"],
    )?;

    let repo_name = docker::exec_in_container_output(
        container_name,
        &["gh", "repo", "view", "--json", "nameWithOwner", "-q", ".nameWithOwner"],
    )?;

    // Link Vercel
    ui::step(4, 4, "Linking to Vercel for auto-deploys...");
    docker::exec_in_container(container_name, &["npx", "vercel", "link", "--yes"])?;
    docker::exec_in_container(container_name, &["npx", "vercel", "--prod", "--yes"])?;

    // Update config
    config.local_only = false;
    config.github_repo = Some(repo_name);
    config.vercel_project = Some(project_name.to_string());
    config.save(cwd)?;

    ui::success("Cloud services connected.");
    Ok(())
}

fn get_production_url(container_name: &str, config: &SpawnConfig) -> Option<String> {
    // Try to get the production URL from Vercel
    if let Ok(output) = docker::exec_in_container_output(
        container_name,
        &["npx", "vercel", "inspect", "--json"],
    ) {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&output) {
            if let Some(url) = parsed.get("url").and_then(|v| v.as_str()) {
                return Some(format!("https://{url}"));
            }
        }
    }

    // Fall back to project name convention
    config
        .vercel_project
        .as_ref()
        .map(|p| format!("https://{p}.vercel.app"))
}
