use crate::cli::RunTool;
use crate::config::SpawnConfig;
use crate::docker::{BollardRuntime, ContainerRuntime};
use crate::error::Result;
use crate::output::Output;

pub async fn run(tool: RunTool, output: &Output) -> Result<()> {
    match tool {
        RunTool::Claude => run_claude(output).await,
    }
}

async fn run_claude(output: &Output) -> Result<()> {
    // Load config
    let config = SpawnConfig::load(None)?;

    // Connect to Docker
    let runtime = BollardRuntime::connect()?;
    runtime.ensure_running().await?;

    let container_id = &config.container.container_id;

    // Check if container is running, restart if stopped
    if !runtime.is_container_running(container_id).await? {
        output.stream_line("Container stopped — restarting...");
        runtime.start_container(container_id).await?;
        output.success("Container restarted");
    }

    // Launch Claude Code interactively
    output.stream_line("Launching Claude Code...");
    output.stream_line("(Press Ctrl+C to exit the session)");

    let exit_code = runtime
        .exec_interactive(
            container_id,
            vec!["claude", "--dangerously-skip-permissions"],
        )
        .await?;

    if exit_code == 0 {
        output.success("Claude Code session ended.");
    } else {
        output.warn(&format!("Claude Code exited with code {exit_code}"));
    }

    output.next_step("Run `spawn run claude` to start another session, or `spawn preview` to create a preview deployment.");

    Ok(())
}
