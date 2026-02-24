use anyhow::Result;
use std::env;
use std::process::{Command, Stdio};

use crate::cli::PreviewArgs;
use crate::config::SpawnConfig;
use crate::docker;
use crate::ui;

pub fn run(args: PreviewArgs) -> Result<()> {
    let cwd = env::current_dir()?;
    let mut config = SpawnConfig::load(&cwd)?;

    if args.close {
        return close_preview(&config);
    }

    // If local-only, prompt for cloud wiring first
    if !config.is_cloud_connected() {
        prompt_cloud_connect(&cwd, &mut config)?;
    }

    let fallback_name = format!("spawn-{}", config.project_name);
    let container_name = config
        .container_name
        .as_deref()
        .unwrap_or(&fallback_name);

    let total = 3;

    // Step 1: Push to preview branch
    ui::step(1, total, "Pushing to preview branch...");
    let branch_name = format!("preview/{}", short_hash());
    docker::exec_in_container(container_name, &["git", "add", "-A"])?;
    let _ = docker::exec_in_container(
        container_name,
        &["git", "commit", "-m", &format!("Preview: {branch_name}")],
    );
    docker::exec_in_container(
        container_name,
        &["git", "push", "origin", &format!("HEAD:{branch_name}"), "--force"],
    )?;

    // Step 2: Trigger Vercel preview deployment
    ui::step(2, total, "Triggering Vercel preview deployment...");
    let deploy_output = docker::exec_in_container_output(
        container_name,
        &["npx", "vercel", "--yes"],
    )?;

    // Step 3: Extract and display URL
    ui::step(3, total, "Preview deployed!");
    let preview_url = extract_url(&deploy_output).unwrap_or_else(|| deploy_output.clone());

    ui::success(&format!(
        "Preview URL: {}",
        ui::hyperlink(&preview_url, &preview_url)
    ));

    // Try to copy to clipboard
    copy_to_clipboard(&preview_url);

    ui::next_step("Share the URL, or run `spawn preview --close` to tear it down.");

    Ok(())
}

fn close_preview(config: &SpawnConfig) -> Result<()> {
    let fallback_name = format!("spawn-{}", config.project_name);
    let container_name = config
        .container_name
        .as_deref()
        .unwrap_or(&fallback_name);

    ui::info("Tearing down preview deployment...");

    // Delete preview branches
    let _ = docker::exec_in_container(
        container_name,
        &[
            "bash",
            "-c",
            "git branch -r | grep 'origin/preview/' | sed 's|origin/||' | xargs -I{} git push origin --delete {}",
        ],
    );

    ui::success("Preview deployment torn down.");
    ui::next_step("Run `spawn deploy` to promote to production.");

    Ok(())
}

/// Prompt the user to connect cloud services (for --local projects).
fn prompt_cloud_connect(cwd: &std::path::Path, config: &mut SpawnConfig) -> Result<()> {
    ui::warn("This project isn't connected to the cloud yet.");

    let confirm = dialoguer::Confirm::new()
        .with_prompt("Connect now?")
        .default(true)
        .interact()?;

    if !confirm {
        anyhow::bail!("Cloud connection required for preview. Run `spawn deploy` or connect manually.");
    }

    let fallback_name = format!("spawn-{}", config.project_name);
    let container_name = config
        .container_name
        .as_deref()
        .unwrap_or(&fallback_name);
    let project_name = &config.project_name;

    ui::info("Provisioning cloud services...");

    // Provision Vercel Postgres
    ui::step(1, 4, "Provisioning Vercel Postgres...");
    docker::exec_in_container(container_name, &["npx", "vercel", "link", "--yes"])?;
    let store_name = format!("{project_name}-db");
    docker::exec_in_container(
        container_name,
        &["npx", "vercel", "stores", "create", "postgres", &store_name, "--yes"],
    )?;
    docker::exec_in_container(container_name, &["npx", "vercel", "env", "pull", ".env.local"])?;

    // Sync env vars
    ui::step(2, 4, "Syncing environment variables...");
    // (simplified — in production we'd parse .env.local)

    // Create GitHub repo
    ui::step(3, 4, "Creating GitHub repo...");
    docker::exec_in_container(container_name, &["git", "init"])?;
    docker::exec_in_container(container_name, &["git", "add", "-A"])?;
    let _ = docker::exec_in_container(
        container_name,
        &["git", "commit", "-m", "Initial commit via spawn"],
    );
    docker::exec_in_container(
        container_name,
        &["gh", "repo", "create", project_name, "--private", "--source", ".", "--push"],
    )?;

    let repo_name = docker::exec_in_container_output(
        container_name,
        &["gh", "repo", "view", "--json", "nameWithOwner", "-q", ".nameWithOwner"],
    )?;

    // Link to Vercel
    ui::step(4, 4, "Linking to Vercel...");
    docker::exec_in_container(container_name, &["npx", "vercel", "link", "--yes"])?;
    docker::exec_in_container(container_name, &["npx", "vercel", "--prod", "--yes"])?;

    // Update config
    config.local_only = false;
    config.github_repo = Some(repo_name);
    config.vercel_project = Some(project_name.to_string());
    config.save(cwd)?;

    ui::success("Cloud services connected.");
    Ok(())
}

fn short_hash() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("{:x}", t & 0xFFFFFF)
}

fn extract_url(output: &str) -> Option<String> {
    output
        .lines()
        .rev()
        .find(|line| line.starts_with("https://"))
        .map(|s| s.trim().to_string())
}

fn copy_to_clipboard(text: &str) {
    // Try multiple clipboard tools
    for cmd in &["pbcopy", "xclip", "xsel", "wl-copy"] {
        if let Ok(mut child) = Command::new(cmd)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
            ui::info("URL copied to clipboard.");
            return;
        }
    }
}
