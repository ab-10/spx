use anyhow::{bail, Result};
use std::env;

use crate::cli::RunArgs;
use crate::config::SpawnConfig;
use crate::docker;
use crate::ui;

pub fn run(args: RunArgs) -> Result<()> {
    match args.tool.as_str() {
        "claude" => run_claude()?,
        other => bail!(
            "Unknown tool: '{other}'. Available tools: claude"
        ),
    }
    Ok(())
}

fn run_claude() -> Result<()> {
    let cwd = env::current_dir()?;
    let config = SpawnConfig::load(&cwd)?;
    let fallback_name = format!("spawn-{}", config.project_name);
    let container_name = config
        .container_name
        .as_deref()
        .unwrap_or(&fallback_name);

    // Ensure the container is running
    ensure_container_running(container_name, &config)?;

    ui::info("Launching Claude Code session inside the container...");
    ui::info("The agent has access to:");
    ui::info(&format!(
        "  {} — Next.js dev server with hot reload",
        ui::hyperlink("http://localhost:3000", "localhost:3000")
    ));
    ui::info("  npm test — pre-configured Playwright suite");
    ui::info("  Full filesystem, git, and Vercel CLI access");
    ui::info("  Stack Auth — stackServerApp.getUser() works immediately");
    eprintln!();

    // Launch Claude Code in dangerous/auto-approve mode
    docker::exec_interactive(
        container_name,
        &["claude", "--dangerously-skip-permissions"],
    )?;

    ui::success("Claude Code session ended.");
    ui::info("spawn does not auto-commit. You handle git.");
    ui::next_step("Run `git diff` to review changes, then `spawn preview` or `spawn deploy`.");

    Ok(())
}

fn ensure_container_running(container_name: &str, _config: &SpawnConfig) -> Result<()> {
    if docker::container_is_running(container_name)? {
        return Ok(());
    }

    if docker::container_exists(container_name)? {
        ui::info("Container exists but is stopped. Starting it...");
        docker::start_container(container_name)?;
        // Restart the dev server
        let _ = docker::exec_in_container(container_name, &["bash", "-c", "npm run dev &"]);
        return Ok(());
    }

    // Container doesn't exist — recreate it
    ui::warn("Container not found. Recreating...");
    let cwd = env::current_dir()?;
    let project_dir = cwd
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("project path is not valid UTF-8"))?;

    docker::create_container(project_dir, container_name)?;
    // Restart the dev server
    let _ = docker::exec_in_container(container_name, &["bash", "-c", "npm run dev &"]);

    Ok(())
}
