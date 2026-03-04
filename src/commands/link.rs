use anyhow::{bail, Context, Result};
use std::env;
use std::path::Path;

use crate::cli::LinkArgs;
use crate::config::SpawnConfig;
use crate::docker;
use crate::host;
use crate::ui;

pub fn run(_args: LinkArgs) -> Result<()> {
    let cwd = env::current_dir()?;

    // --- Preconditions ---
    let mut config = SpawnConfig::load(&cwd)
        .context("No spawn.config.json found. Run `spawn new` first.")?;

    if !cwd.join(".git").exists() {
        bail!("No .git directory found. Run `spawn new` to create a project first.");
    }

    host::require_tool(
        "gh",
        "https://cli.github.com — `brew install gh` or `apt install gh`",
    )?;
    host::require_tool(
        "vercel",
        "https://vercel.com/docs/cli — `npm i -g vercel`",
    )?;

    let container_name = config
        .container_name
        .clone()
        .unwrap_or_else(|| format!("spawn-{}", config.project_name));
    let project_name = config.project_name.clone();

    let total = 8;

    // Step 1: GitHub auth
    ui::step(1, total, "Checking GitHub authentication...");
    if !host::check_auth("gh", &["auth", "status"]) {
        ui::info("Not logged in to GitHub. Starting login...");
        host::run_interactive("gh", &["auth", "login"])?;
    }
    ui::success("GitHub authenticated.");

    // Step 2: Vercel auth
    ui::step(2, total, "Checking Vercel authentication...");
    if !host::check_auth("vercel", &["whoami"]) {
        ui::info("Not logged in to Vercel. Starting login...");
        host::run_interactive("vercel", &["login"])?;
    }
    ui::success("Vercel authenticated.");

    // Step 3: Create GitHub repo
    ui::step(3, total, "Creating GitHub repository...");
    host::run_streaming(
        "gh",
        &[
            "repo",
            "create",
            &project_name,
            "--private",
            "--source=.",
            "--push",
        ],
    )?;

    // Read back the GitHub repo (owner/repo) from the remote
    let remote_url = host::run_capture("git", &["remote", "get-url", "origin"])?;
    let github_repo = parse_github_repo(&remote_url)
        .unwrap_or_else(|| project_name.clone());
    ui::success(&format!("GitHub repo created: {github_repo}"));

    // Step 4: Create Vercel project + first deploy
    ui::step(4, total, "Creating Vercel project and deploying...");
    host::run_streaming("vercel", &["--yes"])?;

    // Step 5: Connect GitHub repo to Vercel for CD
    ui::step(5, total, "Connecting GitHub to Vercel for continuous deployment...");
    connect_github_to_vercel(&cwd, &github_repo)?;
    ui::success("Continuous deployment configured.");

    // Step 6: Sync env vars to Vercel
    ui::step(6, total, "Syncing environment variables to Vercel...");
    sync_env_vars(&cwd)?;

    // Step 7: Sync auth tokens to container
    ui::step(7, total, "Syncing auth tokens to container...");
    sync_tokens_to_container(&container_name)?;

    // Step 8: Update config
    ui::step(8, total, "Updating configuration...");
    config.github_repo = Some(github_repo.clone());
    config.save(&cwd)?;

    ui::success("Project linked successfully!");
    ui::info(&format!(
        "GitHub: {}",
        ui::hyperlink(
            &format!("https://github.com/{github_repo}"),
            &github_repo,
        )
    ));
    ui::info("Vercel: deployment triggered on every push to main.");

    ui::next_step("Run `spawn claude` to start building with Claude Code.");

    Ok(())
}

/// Parse "owner/repo" from a GitHub remote URL.
fn parse_github_repo(remote_url: &str) -> Option<String> {
    let url = remote_url.trim();
    // SSH format: git@github.com:owner/repo.git
    if let Some(rest) = url.strip_prefix("git@github.com:") {
        return Some(rest.trim_end_matches(".git").to_string());
    }
    // HTTPS format: https://github.com/owner/repo.git
    if let Some(rest) = url.strip_prefix("https://github.com/") {
        return Some(rest.trim_end_matches(".git").to_string());
    }
    None
}

/// Read .vercel/project.json to get projectId and orgId, then call Vercel API
/// to link the GitHub repo for continuous deployment.
fn connect_github_to_vercel(cwd: &Path, github_repo: &str) -> Result<()> {
    let project_json_path = cwd.join(".vercel").join("project.json");
    let project_json = std::fs::read_to_string(&project_json_path)
        .context("Could not read .vercel/project.json — did `vercel --yes` succeed?")?;

    let project: serde_json::Value = serde_json::from_str(&project_json)
        .context("Failed to parse .vercel/project.json")?;

    let project_id = project["projectId"]
        .as_str()
        .context("projectId not found in .vercel/project.json")?;

    let token = read_vercel_token()?;

    let body = serde_json::json!({
        "link": {
            "type": "github",
            "repo": github_repo,
            "productionBranch": "main"
        }
    });

    let url = format!("https://api.vercel.com/v9/projects/{project_id}");

    let resp = ureq::patch(&url)
        .set("Authorization", &format!("Bearer {token}"))
        .set("Content-Type", "application/json")
        .send_string(&body.to_string());

    match resp {
        Ok(r) => {
            let status = r.status();
            if status >= 400 {
                let body = r.into_string().unwrap_or_default();
                bail!("Vercel API error ({status}): {body}");
            }
        }
        Err(ureq::Error::Status(code, response)) => {
            let body = response.into_string().unwrap_or_default();
            bail!("Vercel API error ({code}): {body}");
        }
        Err(e) => {
            bail!("Failed to call Vercel API: {e}");
        }
    }

    Ok(())
}

/// Read the Vercel auth token from the CLI's auth file.
fn read_vercel_token() -> Result<String> {
    let auth_path = vercel_auth_path()?;
    let contents = std::fs::read_to_string(&auth_path)
        .with_context(|| format!("Could not read Vercel auth file at {}", auth_path.display()))?;
    let auth: serde_json::Value = serde_json::from_str(&contents)
        .context("Failed to parse Vercel auth file")?;

    // The auth file has a "token" field
    auth["token"]
        .as_str()
        .map(|s| s.to_string())
        .context("No token found in Vercel auth file")
}

/// Return the platform-specific path to the Vercel CLI auth file.
fn vercel_auth_path() -> Result<std::path::PathBuf> {
    let home = env::var("HOME").context("HOME not set")?;

    if cfg!(target_os = "macos") {
        Ok(std::path::PathBuf::from(home)
            .join("Library/Application Support/com.vercel.cli/auth.json"))
    } else {
        Ok(std::path::PathBuf::from(home)
            .join(".local/share/com.vercel.cli/auth.json"))
    }
}

/// Return the platform-specific directory containing Vercel CLI config.
fn vercel_config_dir() -> Result<std::path::PathBuf> {
    let home = env::var("HOME").context("HOME not set")?;

    if cfg!(target_os = "macos") {
        Ok(std::path::PathBuf::from(home)
            .join("Library/Application Support/com.vercel.cli"))
    } else {
        Ok(std::path::PathBuf::from(home)
            .join(".local/share/com.vercel.cli"))
    }
}

/// Parse .env.local and sync each variable to Vercel for all environments.
fn sync_env_vars(cwd: &Path) -> Result<()> {
    let env_file = cwd.join(".env.local");
    if !env_file.exists() {
        ui::info("No .env.local found — skipping env var sync.");
        return Ok(());
    }

    let contents = std::fs::read_to_string(&env_file)?;
    let mut count = 0;

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            // Pipe value via stdin to avoid shell escaping issues
            host::run_with_stdin(
                "vercel",
                &["env", "add", key, "production", "preview", "development"],
                value,
            )?;
            count += 1;
        }
    }

    ui::success(&format!("Synced {count} environment variable(s) to Vercel."));
    Ok(())
}

/// Copy gh and vercel auth configs from host into the container.
fn sync_tokens_to_container(container_name: &str) -> Result<()> {
    let home = env::var("HOME").context("HOME not set")?;

    // Sync GitHub CLI config
    let gh_config = format!("{home}/.config/gh");
    if std::path::Path::new(&gh_config).exists() {
        docker::copy_to_container(container_name, &gh_config, "/home/claude/.config/gh")?;
        docker::exec_in_container(
            container_name,
            &["chown", "-R", "claude:claude", "/home/claude/.config/gh"],
        )?;
    }

    // Sync Vercel CLI config
    let vercel_dir = vercel_config_dir()?;
    if vercel_dir.exists() {
        let vercel_dir_str = vercel_dir
            .to_str()
            .context("Vercel config path is not valid UTF-8")?;
        docker::copy_to_container(
            container_name,
            vercel_dir_str,
            "/home/claude/.local/share/com.vercel.cli",
        )?;
        docker::exec_in_container(
            container_name,
            &[
                "chown",
                "-R",
                "claude:claude",
                "/home/claude/.local/share/com.vercel.cli",
            ],
        )?;
    }

    ui::success("Auth tokens synced to container.");
    Ok(())
}
