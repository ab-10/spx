use anyhow::{bail, Context, Result};
use std::env;
use std::path::PathBuf;

use crate::cli::NewArgs;
use crate::config::{ensure_gitignore_has_spawn, LocalState, SpawnConfig};
use crate::runtime;
use crate::ui;

pub fn run(args: NewArgs, verbose: bool) -> Result<()> {
    let project_name = &args.name;
    let project_dir = env::current_dir()?.join(project_name);

    if verbose {
        ui::verbose(&format!("Project directory: {}", project_dir.display()));
    }

    if SpawnConfig::exists(&project_dir) {
        bail!(
            "Project '{}' already exists. Config found at {}",
            project_name,
            SpawnConfig::path(&project_dir).display()
        );
    }

    let mut state = LocalState::init(project_name);
    let container_name = state.container_name.clone();
    if verbose {
        ui::verbose(&format!("Container name: {container_name}"));
    }

    let total = 6;

    // Step 1: Container image
    ui::step(1, total, "Pulling spawn base container image...");
    if verbose {
        ui::verbose("Checking Apple Container availability...");
    }
    runtime::ensure_available()?;
    if verbose {
        ui::verbose("Pulling base image...");
    }
    if runtime::pull_base_image().is_err() {
        if verbose {
            ui::verbose("Pull failed, checking for local image...");
        }
        runtime::build_base_image_if_missing()?;
    }

    // Step 2: Create project directory and scaffold Next.js app
    ui::step(2, total, "Scaffolding Next.js app...");
    clean_leftover_project_dir(&project_dir)?;
    std::fs::create_dir_all(&project_dir)?;
    let project_dir_str = project_dir
        .to_str()
        .context("project path is not valid UTF-8")?;

    // Remove any existing container with same name
    runtime::remove_container(&container_name)?;

    if verbose {
        ui::verbose("Creating container...");
    }
    let result = runtime::create_container(project_dir_str, &container_name)?;
    if verbose {
        ui::verbose(&format!("Container {} created.", result.container_id));
    }

    // Scaffold Next.js inside the container
    if verbose {
        ui::verbose("Running create-next-app inside container...");
    }
    runtime::exec_in_container(
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

    state.container_id = Some(result.container_id);
    state.container_ip = Some(result.container_ip);
    state.save(&project_dir)?;

    ensure_gitignore_has_spawn(&project_dir)?;

    // Step 4: Git init
    ui::step(4, total, "Initializing git repository...");
    runtime::exec_in_container_as(&container_name, &["git", "init"], "claude")?;
    runtime::exec_in_container_as(
        &container_name,
        &["git", "config", "user.email", "spawn@localhost"],
        "claude",
    )?;
    runtime::exec_in_container_as(
        &container_name,
        &["git", "config", "user.name", "spawn"],
        "claude",
    )?;

    // Step 5: Initial commit
    ui::step(5, total, "Creating initial commit...");
    runtime::exec_in_container_as(&container_name, &["git", "add", "-A"], "claude")?;
    runtime::exec_in_container_as(
        &container_name,
        &["git", "commit", "-m", "Initial commit from spawn"],
        "claude",
    )?;

    // Step 6: Start dev server
    ui::step(6, total, "Starting dev server...");
    if verbose {
        ui::verbose("Running: npm run dev &");
    }
    runtime::exec_in_container(&container_name, &["bash", "-c", "npm run dev &"])?;

    ui::success(&format!("Project '{project_name}' created."));

    if let Some(url_label) = state.dev_url() {
        let url = format!("http://{url_label}");
        ui::info(&format!(
            "Dev server running at {}",
            ui::hyperlink(&url, &url_label)
        ));
    }

    ui::next_step(&format!(
        "Run `cd {project_name} && spawn claude` to start an agent session."
    ));

    Ok(())
}

/// If the project directory exists and has files but no config, a previous run
/// must have crashed partway through. Wipe the leftovers so scaffolding can
/// start fresh.
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
