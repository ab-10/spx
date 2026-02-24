use anyhow::{bail, Context, Result};
use std::env;
use std::path::PathBuf;

use crate::cli::InitArgs;
use crate::config::SpawnConfig;
use crate::docker;
use crate::ui;

pub fn run(args: InitArgs) -> Result<()> {
    let project_name = resolve_project_name(&args.name)?;
    let project_dir = env::current_dir()?.join(&project_name);
    let container_name = format!("spawn-{project_name}");

    if SpawnConfig::exists(&project_dir) {
        bail!(
            "Project '{}' already initialized. Config found at {}",
            project_name,
            SpawnConfig::path(&project_dir).display()
        );
    }

    if args.local {
        run_local(&project_name, &project_dir, &container_name, args.non_interactive)?;
    } else {
        run_cloud(&project_name, &project_dir, &container_name, args.non_interactive)?;
    }

    Ok(())
}

/// Resolve the project name: use the provided name or fall back to the current directory name.
fn resolve_project_name(name: &Option<String>) -> Result<String> {
    match name {
        Some(n) => Ok(n.clone()),
        None => {
            let cwd = env::current_dir()?;
            cwd.file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
                .context("could not determine project name from current directory")
        }
    }
}

/// --local mode: scaffold only, no cloud wiring.
fn run_local(project_name: &str, project_dir: &PathBuf, container_name: &str, non_interactive: bool) -> Result<()> {
    let total = 4;

    // Step 1: Docker image
    ui::step(1, total, "Pulling spawn base Docker image...");
    docker::ensure_docker()?;
    if let Err(_) = docker::pull_base_image() {
        docker::build_base_image_if_missing()?;
    }

    // Step 2: Create project directory and scaffold Next.js app
    ui::step(2, total, "Scaffolding Next.js app...");
    std::fs::create_dir_all(project_dir)?;
    let project_dir_str = project_dir
        .to_str()
        .context("project path is not valid UTF-8")?;

    // Remove any existing container with same name
    docker::remove_container(container_name)?;

    let container_id = docker::create_container(project_dir_str, container_name)?;

    // Scaffold Next.js inside the container
    docker::exec_in_container(
        container_name,
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
        ],
    )?;

    // Step 3: Install Stack Auth
    ui::step(3, total, "Installing Stack Auth (no-browser mode)...");
    docker::exec_in_container(
        container_name,
        &["npx", "@stackframe/init-stack", "--no-browser"],
    )?;

    // Start the dev server in the background
    docker::exec_in_container(container_name, &["bash", "-c", "npm run dev &"])?;

    // Step 4: Save config and drop into container
    ui::step(4, total, "Saving configuration...");
    let config = SpawnConfig {
        project_name: project_name.to_string(),
        local_only: true,
        container_id: Some(container_id),
        container_name: Some(container_name.to_string()),
        ..Default::default()
    };
    config.save(project_dir)?;

    ui::success(&format!("Project '{project_name}' initialized (local mode)."));
    ui::info("Auth pages work locally. Env vars are placeholders until cloud wiring.");
    ui::info(&format!(
        "Dev server: {}",
        ui::hyperlink("http://localhost:3000", "http://localhost:3000")
    ));

    ui::next_step(&format!("Run `spawn run claude` to start an agent session, or `spawn deploy` to connect to the cloud."));

    if !non_interactive {
        // Drop into container shell
        ui::info("Dropping you into the container...");
        docker::attach_shell(container_name)?;
    }

    Ok(())
}

/// Default mode: full cloud-connected setup.
fn run_cloud(project_name: &str, project_dir: &PathBuf, container_name: &str, non_interactive: bool) -> Result<()> {
    let total = 7;

    // Step 1: Docker image
    ui::step(1, total, "Pulling spawn base Docker image...");
    docker::ensure_docker()?;
    if let Err(_) = docker::pull_base_image() {
        docker::build_base_image_if_missing()?;
    }

    // Step 2: Scaffold Next.js app
    ui::step(2, total, "Scaffolding Next.js app with TypeScript, Tailwind, App Router...");
    std::fs::create_dir_all(project_dir)?;
    let project_dir_str = project_dir
        .to_str()
        .context("project path is not valid UTF-8")?;

    docker::remove_container(container_name)?;
    let container_id = docker::create_container(project_dir_str, container_name)?;

    docker::exec_in_container(
        container_name,
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
        ],
    )?;

    // Step 3: Provision Vercel Postgres
    ui::step(3, total, "Provisioning Vercel Postgres...");
    provision_vercel_postgres(container_name, project_name)?;

    // Step 4: Install Stack Auth
    ui::step(4, total, "Installing Stack Auth...");
    docker::exec_in_container(
        container_name,
        &["npx", "@stackframe/init-stack", "--no-browser"],
    )?;

    // Step 5: Sync env vars to Vercel
    ui::step(5, total, "Syncing environment variables to Vercel...");
    sync_env_to_vercel(container_name)?;

    // Step 6: Create GitHub repo and link to Vercel
    ui::step(6, total, "Creating GitHub repo and linking to Vercel...");
    let github_repo = setup_github_and_vercel(container_name, project_name)?;

    // Start the dev server in the background
    docker::exec_in_container(container_name, &["bash", "-c", "npm run dev &"])?;

    // Step 7: Save config
    ui::step(7, total, "Saving configuration...");
    let config = SpawnConfig {
        project_name: project_name.to_string(),
        local_only: false,
        github_repo: Some(github_repo.clone()),
        vercel_project: Some(project_name.to_string()),
        container_id: Some(container_id),
        container_name: Some(container_name.to_string()),
        ..Default::default()
    };
    config.save(project_dir)?;

    ui::success(&format!("Project '{project_name}' fully initialized."));
    ui::info(&format!(
        "GitHub: {}",
        ui::hyperlink(
            &format!("https://github.com/{github_repo}"),
            &github_repo
        )
    ));
    ui::info(&format!(
        "Dev server: {}",
        ui::hyperlink("http://localhost:3000", "http://localhost:3000")
    ));

    ui::next_step(&format!(
        "Run `spawn run claude` to start an agent session."
    ));

    if !non_interactive {
        // Drop into container
        ui::info("Dropping you into the container...");
        docker::attach_shell(container_name)?;
    }

    Ok(())
}

/// Provision Vercel Postgres via the Vercel CLI.
fn provision_vercel_postgres(container_name: &str, project_name: &str) -> Result<()> {
    // Link or create the Vercel project first
    docker::exec_in_container(
        container_name,
        &["npx", "vercel", "link", "--yes"],
    )?;

    // Create Postgres storage
    let store_name = format!("{project_name}-db");
    docker::exec_in_container(
        container_name,
        &[
            "npx",
            "vercel",
            "stores",
            "create",
            "postgres",
            &store_name,
            "--yes",
        ],
    )?;

    // Pull env vars (includes DATABASE_URL etc.)
    docker::exec_in_container(
        container_name,
        &["npx", "vercel", "env", "pull", ".env.local"],
    )?;

    Ok(())
}

/// Sync .env.local to Vercel for preview + production environments.
fn sync_env_to_vercel(container_name: &str) -> Result<()> {
    // Read the .env.local file from the container
    let env_content = docker::exec_in_container_output(
        container_name,
        &["cat", ".env.local"],
    )?;

    for line in env_content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            // Add to preview environment
            let _ = docker::exec_in_container(
                container_name,
                &[
                    "bash",
                    "-c",
                    &format!(
                        "echo '{}' | npx vercel env add {} preview --yes 2>/dev/null || true",
                        value, key
                    ),
                ],
            );
            // Add to production environment
            let _ = docker::exec_in_container(
                container_name,
                &[
                    "bash",
                    "-c",
                    &format!(
                        "echo '{}' | npx vercel env add {} production --yes 2>/dev/null || true",
                        value, key
                    ),
                ],
            );
        }
    }

    Ok(())
}

/// Create a GitHub repo, push initial commit, and link to Vercel.
fn setup_github_and_vercel(container_name: &str, project_name: &str) -> Result<String> {
    // Initialize git repo
    docker::exec_in_container(container_name, &["git", "init"])?;
    docker::exec_in_container(container_name, &["git", "add", "-A"])?;
    docker::exec_in_container(
        container_name,
        &["git", "commit", "-m", "Initial commit via spawn init"],
    )?;

    // Create GitHub repo via gh CLI
    docker::exec_in_container(
        container_name,
        &[
            "gh",
            "repo",
            "create",
            project_name,
            "--private",
            "--source",
            ".",
            "--push",
        ],
    )?;

    // Get the full repo name (owner/repo)
    let repo_name = docker::exec_in_container_output(
        container_name,
        &["gh", "repo", "view", "--json", "nameWithOwner", "-q", ".nameWithOwner"],
    )?;

    // Link Vercel to the GitHub repo for auto-deploys
    docker::exec_in_container(
        container_name,
        &["npx", "vercel", "link", "--yes"],
    )?;

    // Deploy once to activate the Vercel-GitHub integration
    docker::exec_in_container(
        container_name,
        &["npx", "vercel", "--prod", "--yes"],
    )?;

    Ok(repo_name)
}
