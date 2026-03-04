use anyhow::{Context, Result};
use std::env;
use std::path::Path;

use crate::cli::LinkArgs;
use crate::config::{migrate_if_needed, recover_config, LocalState, SpawnConfig};
use crate::docker;
use crate::ui;

pub fn run(_args: LinkArgs) -> Result<()> {
    let cwd = env::current_dir()?;

    migrate_if_needed(&cwd)?;

    let config = if SpawnConfig::exists(&cwd) {
        SpawnConfig::load(&cwd)
            .context("Failed to load spawn.config.json. Run `spawn new` first.")?
    } else {
        recover_config(&cwd)?
    };

    let mut state = if LocalState::exists(&cwd) {
        LocalState::load(&cwd)?
    } else {
        let s = LocalState::init(&config.project_name);
        s.save(&cwd)?;
        s
    };

    let container_name = state.container_name.clone();
    let project_name = config.project_name.clone();

    super::ensure_container_running(&container_name, &mut state, &cwd)?;

    let total = 6;

    // Step 1: GitHub auth (inside container)
    ui::step(1, total, "Checking GitHub authentication...");
    if !docker::check_in_container_as(&container_name, &["gh", "auth", "status"], "claude") {
        ui::info("Not logged in to GitHub. Starting login...");
        docker::exec_interactive(&container_name, &["gh", "auth", "login", "-h", "github.com"], Some("claude"))?;
    }
    ui::success("GitHub authenticated.");

    // Step 2: Vercel auth (inside container)
    ui::step(2, total, "Checking Vercel authentication...");
    if !docker::check_in_container_as(&container_name, &["vercel", "whoami"], "claude") {
        ui::info("Not logged in to Vercel. Starting login...");
        docker::exec_interactive(&container_name, &["vercel", "login"], Some("claude"))?;
    }
    ui::success("Vercel authenticated.");

    // Step 3: Create GitHub repo (inside container)
    ui::step(3, total, "Creating GitHub repository...");
    docker::exec_in_container_as(
        &container_name,
        &[
            "gh", "repo", "create", &project_name,
            "--private", "--source=.", "--push",
        ],
        "claude",
    )?;

    // Read back the GitHub repo (owner/repo) from the remote
    let remote_url = docker::exec_capture_in_container_as(
        &container_name,
        &["git", "remote", "get-url", "origin"],
        "claude",
    )?;
    let github_repo = parse_github_repo(&remote_url)
        .unwrap_or_else(|| project_name.clone());
    ui::success(&format!("GitHub repo created: {github_repo}"));

    // Step 4: Create Vercel project + first deploy (inside container)
    ui::step(4, total, "Creating Vercel project and deploying...");
    docker::exec_in_container_as(&container_name, &["vercel", "--yes"], "claude")?;

    // Step 5: Connect GitHub repo to Vercel for CD (inside container)
    ui::step(5, total, "Connecting GitHub to Vercel for continuous deployment...");
    connect_github_to_vercel_in_container(&container_name, &github_repo)?;
    ui::success("Continuous deployment configured.");

    // Step 6: Sync env vars to Vercel
    ui::step(6, total, "Syncing environment variables to Vercel...");
    sync_env_vars(&container_name, &cwd)?;

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

/// Connect GitHub repo to Vercel for CD by running a bash script inside the container.
fn connect_github_to_vercel_in_container(container_name: &str, github_repo: &str) -> Result<()> {
    let script = format!(
        r#"
TOKEN=$(node -e "process.stdout.write(require('/home/claude/.local/share/com.vercel.cli/auth.json').token)")
PROJECT_ID=$(node -e "process.stdout.write(require('/app/.vercel/project.json').projectId)")

curl -sf -X PATCH "https://api.vercel.com/v9/projects/$PROJECT_ID" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{{"link":{{"type":"github","repo":"{github_repo}","productionBranch":"main"}}}}'
"#
    );

    docker::exec_in_container_as(
        container_name,
        &["bash", "-c", script.trim()],
        "claude",
    )
}

/// Parse .env.local and sync each variable to Vercel for all environments (inside container).
fn sync_env_vars(container_name: &str, cwd: &Path) -> Result<()> {
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
            docker::exec_with_stdin_in_container(
                container_name,
                &["vercel", "env", "add", key, "production", "preview", "development"],
                value,
            )?;
            count += 1;
        }
    }

    ui::success(&format!("Synced {count} environment variable(s) to Vercel."));
    Ok(())
}
