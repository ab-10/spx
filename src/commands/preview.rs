use anyhow::Result;
use serde::Serialize;
use std::process::Stdio;
use tokio::process::Command;

use crate::cloud;
use crate::config::SpawnConfig;
use crate::output;

#[derive(Serialize)]
struct PreviewOutput {
    url: Option<String>,
    branch: Option<String>,
    status: String,
    next_step: String,
}

pub async fn run(close: bool, json: bool) -> Result<()> {
    let (mut config, project_dir) = SpawnConfig::find()?;

    if close {
        return close_preview(&config, &project_dir, json).await;
    }

    if !json {
        output::header("Deploying preview");
    }

    // Ensure cloud wiring is in place
    if !cloud::ensure_cloud_connected(&mut config, &project_dir).await? {
        anyhow::bail!("Cloud wiring is required for preview. Aborting.");
    }

    let total_steps = 3;

    // Step 1: Create and push to a preview branch
    if !json {
        output::step(1, total_steps, "Pushing to preview branch...");
    }

    let branch_name = format!(
        "preview/{}",
        chrono_free_timestamp()
    );

    // Create preview branch from current state
    Command::new("git")
        .args(["checkout", "-B", &branch_name])
        .current_dir(&project_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await?;

    Command::new("git")
        .args(["add", "-A"])
        .current_dir(&project_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await?;

    let _ = Command::new("git")
        .args(["commit", "-m", "Preview deployment"])
        .current_dir(&project_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;

    cloud::github::push_branch(&project_dir, &branch_name).await?;

    // Step 2: Trigger Vercel preview deployment
    if !json {
        output::step(2, total_steps, "Triggering Vercel preview deployment...");
    }
    let url = cloud::vercel::deploy_preview(&project_dir).await?;

    // Step 3: Switch back to previous branch
    if !json {
        output::step(3, total_steps, "Switching back to working branch...");
    }
    let _ = Command::new("git")
        .args(["checkout", "-"])
        .current_dir(&project_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;

    if !json {
        println!();
        output::success(&format!(
            "Preview deployed: {}",
            output::hyperlink(&url, &url)
        ));
        output::next_step("Run `spawn preview --close` to tear it down, or `spawn deploy` to go to production.");
    }

    let result = PreviewOutput {
        url: Some(url),
        branch: Some(branch_name),
        status: "deployed".to_string(),
        next_step: "spawn deploy".to_string(),
    };
    output::json_output(json, &result);

    Ok(())
}

async fn close_preview(
    _config: &SpawnConfig,
    project_dir: &std::path::PathBuf,
    json: bool,
) -> Result<()> {
    if !json {
        output::header("Tearing down preview deployment");
    }

    cloud::vercel::remove_preview(project_dir).await?;

    // Clean up preview branches
    let out = Command::new("git")
        .args(["branch", "--list", "preview/*"])
        .current_dir(project_dir)
        .output()
        .await?;

    let branches = String::from_utf8_lossy(&out.stdout);
    for branch in branches.lines() {
        let branch = branch.trim();
        if !branch.is_empty() {
            let _ = Command::new("git")
                .args(["branch", "-D", branch])
                .current_dir(project_dir)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await;
        }
    }

    if !json {
        output::success("Preview deployments torn down.");
        output::next_step("Run `spawn deploy` to push to production.");
    }

    let result = PreviewOutput {
        url: None,
        branch: None,
        status: "closed".to_string(),
        next_step: "spawn deploy".to_string(),
    };
    output::json_output(json, &result);

    Ok(())
}

/// Generate a simple timestamp string without depending on chrono.
fn chrono_free_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{secs}")
}
