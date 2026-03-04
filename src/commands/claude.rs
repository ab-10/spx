use anyhow::Result;
use std::env;
use std::fs;
use std::path::Path;

use crate::cli::ClaudeArgs;
use crate::config::{migrate_if_needed, recover_config, LocalState, SpawnConfig};
use crate::docker;
use crate::ui;

pub fn run(_args: ClaudeArgs, verbose: bool) -> Result<()> {
    let cwd = env::current_dir()?;

    if verbose { ui::verbose(&format!("Working directory: {}", cwd.display())); }

    if verbose { ui::verbose("Running config migration check..."); }
    migrate_if_needed(&cwd)?;

    if verbose { ui::verbose("Loading spawn config..."); }
    let config = if SpawnConfig::exists(&cwd) {
        SpawnConfig::load(&cwd)?
    } else {
        if verbose { ui::verbose("No spawn.config.json found, attempting recovery..."); }
        recover_config(&cwd)?
    };
    if verbose { ui::verbose(&format!("Project: {}", config.project_name)); }

    if verbose { ui::verbose("Loading local state..."); }
    let mut state = if LocalState::exists(&cwd) {
        LocalState::load(&cwd)?
    } else {
        if verbose { ui::verbose("No local state found, initializing..."); }
        let s = LocalState::init(&config.project_name);
        s.save(&cwd)?;
        s
    };

    let container_name = state.container_name.clone();
    if verbose { ui::verbose(&format!("Container: {container_name}")); }

    super::ensure_container_running(&container_name, &mut state, &cwd, verbose)?;

    let host_port = state.port
        .ok_or_else(|| anyhow::anyhow!("No port configured. Re-run `spawn new` to fix."))?;
    let url = format!("http://localhost:{host_port}");
    ui::info("Launching Claude Code session inside the container...");
    ui::info("The agent has access to:");
    ui::info(&format!(
        "  {} — Next.js dev server with hot reload",
        ui::hyperlink(&url, &format!("localhost:{host_port}"))
    ));
    ui::info("  npm test — pre-configured Playwright suite");
    ui::info("  Full filesystem, git, and Vercel CLI access");
    eprintln!();

    // Write AGENTS.md so Claude Code knows the port mapping
    write_agents_md(&cwd, host_port)?;

    if verbose { ui::verbose(&format!("Exec: docker exec -it -u claude {container_name} claude --dangerously-skip-permissions")); }

    // Launch Claude Code as the non-root "claude" user in auto-approve mode
    docker::exec_interactive(
        &container_name,
        &["claude", "--dangerously-skip-permissions"],
        Some("claude"),
    )?;

    ui::success("Claude Code session ended.");
    ui::info("spawn does not auto-commit. You handle git.");
    ui::next_step("Run `git diff` to review changes, then commit and push to deploy.");

    Ok(())
}

/// Write AGENTS.md into the project directory so Claude Code knows the port mapping.
fn write_agents_md(project_dir: &Path, host_port: u16) -> Result<()> {
    let content = format!(
        "\
# Environment

You are running inside a Docker container.

- Run applications on port **3000** (the container port).
- When telling the user how to access the app, use **localhost:{host_port}** (the host port mapped to container port 3000).
"
    );
    fs::write(project_dir.join("AGENTS.md"), content)?;
    Ok(())
}
