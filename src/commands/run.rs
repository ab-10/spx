use anyhow::Result;
use serde::Serialize;

use crate::config::SpawnConfig;
use crate::{docker, output};

#[derive(Serialize)]
struct RunOutput {
    container_name: String,
    command: String,
    status: String,
    next_step: String,
}

pub async fn run(target: String, json: bool) -> Result<()> {
    if target != "claude" {
        anyhow::bail!(
            "Unknown run target: '{target}'. Currently supported: `spawn run claude`"
        );
    }

    let (config, _project_dir) = SpawnConfig::find()?;
    let container_name = &config.container_name;

    if !json {
        output::header("Launching Claude Code session");
    }

    // Ensure container is running
    if !docker::is_container_running(container_name).await? {
        if !json {
            output::step(1, 2, "Starting container...");
        }
        docker::start_container(container_name).await?;
    }

    // Start the dev server if not already running
    let _ = docker::exec_capture(
        container_name,
        &["sh", "-c", "pgrep -f 'next dev' || npm run dev &"],
    )
    .await;

    if !json {
        output::step(2, 2, "Starting Claude Code in dangerous/auto-approve mode...");
        println!();
        output::success("Agent has access to:");
        println!("  • localhost:3000 — Next.js dev server with hot reload");
        println!("  • npm test — Playwright suite in /tests");
        println!("  • Full filesystem, git, and Vercel CLI access");
        if config.stack_auth.is_some() {
            println!("  • Stack Auth — stackServerApp.getUser() works immediately");
        }
        println!();
    }

    // Launch Claude Code interactively
    let result = docker::exec_interactive(
        container_name,
        &["npx", "@anthropic-ai/claude-code", "--dangerously-skip-permissions"],
    )
    .await;

    let status = if result.is_ok() { "exited" } else { "error" };

    if !json {
        println!();
        output::success("Claude Code session ended.");
        output::warn("spawn does not auto-commit. You handle git.");
        output::next_step("Run `spawn preview` to share your work, or `spawn deploy` to go to production.");
    }

    let out = RunOutput {
        container_name: container_name.clone(),
        command: "claude-code".to_string(),
        status: status.to_string(),
        next_step: "spawn preview".to_string(),
    };
    output::json_output(json, &out);

    Ok(())
}
