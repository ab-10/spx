use anyhow::{Context, Result};
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

use crate::config::GithubConfig;
use crate::output;

/// Verify that the GitHub CLI is installed and authenticated.
pub async fn check_cli() -> Result<()> {
    which::which("gh")
        .map_err(|_| anyhow::anyhow!("{}", crate::error::SpawnError::GhCliNotFound))?;

    let out = Command::new("gh")
        .args(["auth", "status"])
        .output()
        .await
        .context("Failed to run `gh auth status`")?;

    if !out.status.success() {
        anyhow::bail!("{}", crate::error::SpawnError::GhNotAuthenticated);
    }
    output::success("GitHub CLI authenticated.");
    Ok(())
}

/// Create a GitHub repository and push the initial commit.
pub async fn create_repo(project_name: &str, project_dir: &Path) -> Result<GithubConfig> {
    // Initialize git if needed
    let git_dir = project_dir.join(".git");
    if !git_dir.exists() {
        Command::new("git")
            .args(["init"])
            .current_dir(project_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await?;
    }

    // Create initial commit if no commits exist
    let has_commits = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(project_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false);

    if !has_commits {
        Command::new("git")
            .args(["add", "-A"])
            .current_dir(project_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await?;

        Command::new("git")
            .args(["commit", "-m", "Initial commit from spawn init"])
            .current_dir(project_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await?;
    }

    // Create repo on GitHub
    let out = Command::new("gh")
        .args([
            "repo",
            "create",
            project_name,
            "--private",
            "--source",
            ".",
            "--push",
        ])
        .current_dir(project_dir)
        .output()
        .await
        .context("Failed to create GitHub repo")?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        // Repo might already exist — try to just set remote
        if stderr.contains("already exists") {
            output::warn("GitHub repo already exists. Setting remote...");
            let owner = get_gh_username().await?;
            let _ = Command::new("git")
                .args([
                    "remote",
                    "add",
                    "origin",
                    &format!("https://github.com/{owner}/{project_name}.git"),
                ])
                .current_dir(project_dir)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await;

            Command::new("git")
                .args(["push", "-u", "origin", "main"])
                .current_dir(project_dir)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await?;

            return Ok(GithubConfig {
                repo: project_name.to_string(),
                owner,
            });
        }
        anyhow::bail!("Failed to create GitHub repo: {stderr}");
    }

    let owner = get_gh_username().await?;
    output::success(&format!(
        "GitHub repo created: {}",
        output::hyperlink(
            &format!("https://github.com/{owner}/{project_name}"),
            &format!("{owner}/{project_name}")
        )
    ));

    Ok(GithubConfig {
        repo: project_name.to_string(),
        owner,
    })
}

/// Push current branch to origin.
pub async fn push_branch(project_dir: &Path, branch: &str) -> Result<()> {
    let status = Command::new("git")
        .args(["push", "-u", "origin", branch])
        .current_dir(project_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .status()
        .await
        .context("Failed to push to GitHub")?;

    if !status.success() {
        anyhow::bail!("Failed to push branch '{branch}' to origin");
    }
    Ok(())
}

/// Get the current GitHub username.
async fn get_gh_username() -> Result<String> {
    let out = Command::new("gh")
        .args(["api", "user", "--jq", ".login"])
        .output()
        .await
        .context("Failed to get GitHub username")?;

    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}
