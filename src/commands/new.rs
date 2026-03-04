use anyhow::{bail, Context, Result};
use std::env;
use std::path::PathBuf;

use crate::cli::NewArgs;
use crate::config::{ensure_gitignore_has_spawn, LocalState, SpawnConfig};
use crate::docker;
use crate::ui;

pub fn run(args: NewArgs) -> Result<()> {
    let project_name = &args.name;
    let project_dir = env::current_dir()?.join(project_name);

    if SpawnConfig::exists(&project_dir) {
        bail!(
            "Project '{}' already exists. Config found at {}",
            project_name,
            SpawnConfig::path(&project_dir).display()
        );
    }

    let mut state = LocalState::init(project_name);
    let container_name = state.container_name.clone();

    let total = 6;

    // Step 1: Docker image
    ui::step(1, total, "Pulling spawn base Docker image...");
    docker::ensure_docker()?;
    if let Err(_) = docker::pull_base_image() {
        docker::build_base_image_if_missing()?;
    }

    // Step 2: Create project directory and scaffold Next.js app
    ui::step(2, total, "Scaffolding Next.js app...");
    clean_leftover_project_dir(&project_dir)?;
    std::fs::create_dir_all(&project_dir)?;
    let project_dir_str = project_dir
        .to_str()
        .context("project path is not valid UTF-8")?;

    // Remove any existing container with same name
    docker::remove_container(&container_name)?;

    let (container_id, port) = docker::create_container_with_fallback(project_dir_str, &container_name)?;

    // Scaffold Next.js inside the container
    docker::exec_in_container(
        &container_name,
        &[
            "npx",
            "create-next-app@latest",
            ".",
            "--typescript",
            "--tailwind",
            "--eslint",
            "--app",
            "--src-dir",
            "--import-alias",
            "@/*",
            "--use-npm",
            "--yes",
        ],
    )?;

    // Step 3: Save config (before git init so it's included in initial commit)
    ui::step(3, total, "Saving configuration...");
    let config = SpawnConfig {
        project_name: project_name.to_string(),
    };
    config.save(&project_dir)?;

    state.container_id = Some(container_id);
    state.port = Some(port);
    state.save(&project_dir)?;

    ensure_gitignore_has_spawn(&project_dir)?;

    // Step 4: Git init
    ui::step(4, total, "Initializing git repository...");
    docker::exec_in_container_as(&container_name, &["git", "init"], "claude")?;
    docker::exec_in_container_as(
        &container_name,
        &["git", "config", "user.email", "spawn@localhost"],
        "claude",
    )?;
    docker::exec_in_container_as(
        &container_name,
        &["git", "config", "user.name", "spawn"],
        "claude",
    )?;

    // Step 5: Initial commit
    ui::step(5, total, "Creating initial commit...");
    docker::exec_in_container_as(&container_name, &["git", "add", "-A"], "claude")?;
    docker::exec_in_container_as(
        &container_name,
        &["git", "commit", "-m", "Initial commit from spawn"],
        "claude",
    )?;

    // Step 6: Start dev server
    ui::step(6, total, "Starting dev server...");
    docker::exec_in_container(&container_name, &["bash", "-c", "npm run dev &"])?;

    ui::success(&format!("Project '{project_name}' created."));

    let host_port = port;
    let url = format!("http://localhost:{host_port}");
    ui::info(&format!(
        "Dev server running at {}",
        ui::hyperlink(&url, &format!("localhost:{host_port}"))
    ));

    ui::next_step(&format!("Run `cd {project_name} && spawn claude` to start an agent session."));

    Ok(())
}

/// If the project directory exists and has files but no config, a previous run
/// must have crashed partway through. Wipe the leftovers so scaffolding can
/// start fresh. The config-file check in `run()` already ran, so we know
/// there is no spawn.config.json.
fn clean_leftover_project_dir(project_dir: &PathBuf) -> Result<()> {
    if !project_dir.exists() {
        return Ok(());
    }
    let has_contents = project_dir
        .read_dir()
        .map(|mut d| d.next().is_some())
        .unwrap_or(false);
    if has_contents {
        ui::warn("Found leftover files from a previous run — cleaning up.");
        std::fs::remove_dir_all(project_dir)
            .context("failed to remove leftover project directory")?;
    }
    Ok(())
}
