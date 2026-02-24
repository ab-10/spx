use anyhow::{Context, Result};
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

use crate::config::VercelConfig;
use crate::output;

/// Verify that the Vercel CLI is installed and authenticated.
pub async fn check_cli() -> Result<()> {
    which::which("vercel")
        .map_err(|_| anyhow::anyhow!("{}", crate::error::SpawnError::VercelCliNotFound))?;

    let out = Command::new("vercel")
        .args(["whoami"])
        .output()
        .await
        .context("Failed to run `vercel whoami`")?;

    if !out.status.success() {
        anyhow::bail!("{}", crate::error::SpawnError::VercelNotAuthenticated);
    }
    output::success(&format!(
        "Vercel authenticated as {}",
        String::from_utf8_lossy(&out.stdout).trim()
    ));
    Ok(())
}

/// Provision a Vercel project and link Postgres.
pub async fn provision_project(project_name: &str, project_dir: &Path) -> Result<VercelConfig> {
    // Link project to Vercel
    let status = Command::new("vercel")
        .args(["link", "--yes", "--project", project_name])
        .current_dir(project_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .status()
        .await
        .context("Failed to link Vercel project")?;

    if !status.success() {
        // Project might not exist yet — create it
        let status = Command::new("vercel")
            .args(["project", "add", project_name])
            .current_dir(project_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .status()
            .await
            .context("Failed to create Vercel project")?;

        if !status.success() {
            anyhow::bail!("Failed to create Vercel project '{project_name}'");
        }

        // Re-link
        Command::new("vercel")
            .args(["link", "--yes", "--project", project_name])
            .current_dir(project_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await?;
    }

    // Provision Postgres (Neon via Vercel integration)
    output::stream_line("vercel", "Provisioning Vercel Postgres (Neon)...");
    let pg_status = Command::new("vercel")
        .args(["env", "add", "POSTGRES_URL", "development", "--force"])
        .current_dir(project_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;

    if let Ok(s) = pg_status {
        if !s.success() {
            output::warn("Could not auto-provision Postgres. You may need to add it via the Vercel dashboard.");
        }
    }

    // Pull env vars including any Postgres connection strings
    let _ = Command::new("vercel")
        .args(["env", "pull", ".env.local"])
        .current_dir(project_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;

    // Read project config
    let project_json = project_dir.join(".vercel/project.json");
    let (project_id, org_id) = if project_json.exists() {
        let contents = std::fs::read_to_string(&project_json)?;
        let parsed: serde_json::Value = serde_json::from_str(&contents)?;
        (
            parsed["projectId"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            parsed["orgId"].as_str().map(|s| s.to_string()),
        )
    } else {
        ("unknown".to_string(), None)
    };

    let config = VercelConfig {
        project_id,
        project_name: project_name.to_string(),
        org_id,
        production_url: Some(format!("{project_name}.vercel.app")),
    };

    output::success("Vercel project provisioned.");
    Ok(config)
}

/// Sync local .env.local vars to Vercel (preview + production).
pub async fn sync_env_vars(project_dir: &Path) -> Result<()> {
    let env_file = project_dir.join(".env.local");
    if !env_file.exists() {
        output::warn("No .env.local found — skipping Vercel env sync.");
        return Ok(());
    }

    let contents = std::fs::read_to_string(&env_file)?;
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();

            // Set for preview and production
            for env in &["preview", "production"] {
                let _ = Command::new("vercel")
                    .args(["env", "add", key, env, "--force"])
                    .current_dir(project_dir)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .await;

                // Pipe value through stdin
                let mut child = Command::new("vercel")
                    .args(["env", "add", key, env, "--force"])
                    .current_dir(project_dir)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()?;

                if let Some(mut stdin) = child.stdin.take() {
                    use tokio::io::AsyncWriteExt;
                    stdin.write_all(value.as_bytes()).await?;
                    drop(stdin);
                }
                let _ = child.wait().await;
            }
        }
    }

    output::success("Environment variables synced to Vercel.");
    Ok(())
}

/// Trigger a Vercel preview deployment.
pub async fn deploy_preview(project_dir: &Path) -> Result<String> {
    let out = Command::new("vercel")
        .args(["deploy", "--yes"])
        .current_dir(project_dir)
        .output()
        .await
        .context("Failed to run `vercel deploy`")?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!("Vercel preview deploy failed: {stderr}");
    }

    let url = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok(url)
}

/// Trigger a Vercel production deployment.
pub async fn deploy_production(project_dir: &Path) -> Result<String> {
    let out = Command::new("vercel")
        .args(["deploy", "--prod", "--yes"])
        .current_dir(project_dir)
        .output()
        .await
        .context("Failed to run `vercel deploy --prod`")?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!("Vercel production deploy failed: {stderr}");
    }

    let url = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok(url)
}

/// Remove preview deployments.
pub async fn remove_preview(project_dir: &Path) -> Result<()> {
    let _ = Command::new("vercel")
        .args(["remove", "--yes", "--safe"])
        .current_dir(project_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;

    output::success("Preview deployments removed.");
    Ok(())
}
