use anyhow::Result;
use std::env;
use std::fs;
use std::path::Path;

use crate::cli::ClaudeArgs;
use crate::config::{migrate_if_needed, recover_config, LocalState, SpawnConfig};
use crate::runtime::{self, Runtime};
use crate::ui;

pub fn run(_args: ClaudeArgs, verbose: bool) -> Result<()> {
    let cwd = env::current_dir()?;

    if verbose {
        ui::verbose(&format!("Working directory: {}", cwd.display()));
    }

    if verbose {
        ui::verbose("Running config migration check...");
    }
    migrate_if_needed(&cwd)?;

    if verbose {
        ui::verbose("Loading spawn config...");
    }
    let config = if SpawnConfig::exists(&cwd) {
        SpawnConfig::load(&cwd)?
    } else {
        if verbose {
            ui::verbose("No spawn.config.json found, attempting recovery...");
        }
        recover_config(&cwd)?
    };
    if verbose {
        ui::verbose(&format!("Project: {}", config.project_name));
    }

    if verbose {
        ui::verbose("Loading local state...");
    }
    let mut state = if LocalState::exists(&cwd) {
        LocalState::load(&cwd)?
    } else {
        if verbose {
            ui::verbose("No local state found, initializing...");
        }
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

    // Write AGENTS.md so Claude Code knows the environment
    write_agents_md(&cwd, &state)?;

    ui::info("Launching Claude Code session inside the container...");
    ui::info("The agent has access to:");
    if let Some(url_label) = state.dev_url() {
        let url = format!("http://{url_label}");
        ui::info(&format!(
            "  {} — Next.js dev server with hot reload",
            ui::hyperlink(&url, &url_label)
        ));
    }
    ui::info("  npm test — pre-configured Playwright suite");
    ui::info("  Full filesystem, git, and Vercel CLI access");
    eprintln!();

    if verbose {
        ui::verbose(&format!(
            "Exec: {} exec -it -u claude {container_name} claude --dangerously-skip-permissions",
            runtime.binary()
        ));
    }

    // Launch Claude Code as the non-root "claude" user in auto-approve mode
    runtime::exec_interactive(
        runtime,
        &container_name,
        &["claude", "--dangerously-skip-permissions"],
        Some("claude"),
    )?;

    ui::success("Claude Code session ended.");
    ui::info("spawn does not auto-commit. You handle git.");
    ui::next_step("Run `git diff` to review changes, then commit and push to deploy.");

    Ok(())
}

/// Write AGENTS.md into the project directory so Claude Code knows the environment.
fn write_agents_md(project_dir: &Path, state: &LocalState) -> Result<()> {
    let runtime = state.runtime();
    let (container_type, access_info) = match runtime {
        Runtime::Docker => {
            let port = state.port.unwrap_or(3000);
            (
                "Docker container",
                format!(
                    "- Run applications on port **3000** (the container port).\n\
                     - When telling the user how to access the app, use **localhost:{port}** (the host port mapped to container port 3000)."
                ),
            )
        }
        Runtime::AppleContainer => {
            let ip = state
                .container_ip
                .as_deref()
                .unwrap_or("unknown");
            (
                "Apple container",
                format!(
                    "- Run applications on port **3000**.\n\
                     - The container has its own IP address: **{ip}**.\n\
                     - When telling the user how to access the app, use **{ip}:3000**."
                ),
            )
        }
    };

    let content = format!(
        "\
# Environment

You are running inside a {container_type}.

{access_info}
"
    );
    fs::write(project_dir.join("AGENTS.md"), content)?;
    Ok(())
}
