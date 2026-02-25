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
    let mut config = SpawnConfig::load(&cwd)?;
    let container_name = config
        .container_name
        .clone()
        .unwrap_or_else(|| format!("spawn-{}", config.project_name));

    // Ensure the container is running
    ensure_container_running(&container_name, &mut config, &cwd)?;

    let port = config.port.unwrap_or(3000);
    let url = format!("http://localhost:{port}");
    ui::info("Launching Claude Code session inside the container...");
    ui::info("The agent has access to:");
    ui::info(&format!(
        "  {} — Next.js dev server with hot reload",
        ui::hyperlink(&url, &format!("localhost:{port}"))
    ));
    ui::info("  npm test — pre-configured Playwright suite");
    ui::info("  Full filesystem, git, and Vercel CLI access");
    ui::info("  Stack Auth — stackServerApp.getUser() works immediately");
    eprintln!();

    // Launch Claude Code as the non-root "claude" user in auto-approve mode
    docker::exec_interactive(
        &container_name,
        &["claude", "--dangerously-skip-permissions"],
        Some("claude"),
    )?;

    ui::success("Claude Code session ended.");
    ui::info("spawn does not auto-commit. You handle git.");
    ui::next_step("Run `git diff` to review changes, then `spawn preview` or `spawn deploy`.");

    Ok(())
}

fn ensure_container_running(container_name: &str, config: &mut SpawnConfig, cwd: &std::path::Path) -> Result<()> {
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
    let project_dir = cwd
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("project path is not valid UTF-8"))?;

    let (_container_id, port) = docker::create_container_with_fallback(project_dir, container_name)?;
    config.port = Some(port);
    config.save(cwd)?;
    // Restart the dev server
    let _ = docker::exec_in_container(container_name, &["bash", "-c", "npm run dev &"]);

    Ok(())
}
