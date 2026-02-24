use anyhow::Result;
use serde::Serialize;
use std::process::Stdio;
use tokio::process::Command;

use crate::cloud;
use crate::config::SpawnConfig;
use crate::{docker, output};

#[derive(Serialize)]
struct DeployOutput {
    production_url: Option<String>,
    tests_passed: bool,
    status: String,
    next_step: String,
}

pub async fn run(force: bool, json: bool) -> Result<()> {
    let (mut config, project_dir) = SpawnConfig::find()?;

    if !json {
        output::header("Deploying to production");
    }

    // Ensure cloud wiring
    if !cloud::ensure_cloud_connected(&mut config, &project_dir).await? {
        anyhow::bail!("Cloud wiring is required for deploy. Aborting.");
    }

    let total_steps = if force { 2 } else { 3 };
    let mut step_num = 0;

    // Step 1: Run tests (unless --force)
    let tests_passed;
    if !force {
        step_num += 1;
        if !json {
            output::step(step_num, total_steps, "Running tests...");
        }
        tests_passed = run_tests(&config).await?;
        if !tests_passed {
            if !json {
                output::error("Tests failed. Fix failing tests before deploying.");
                output::warn("Use `spawn deploy --force` to skip the test gate.");
                output::next_step("Fix tests and run `spawn deploy` again.");
            }
            let result = DeployOutput {
                production_url: None,
                tests_passed: false,
                status: "failed".to_string(),
                next_step: "Fix tests".to_string(),
            };
            output::json_output(json, &result);
            anyhow::bail!("{}", crate::error::SpawnError::TestsFailed);
        }
        if !json {
            output::success("All tests passed.");
        }
    } else {
        tests_passed = true;
        if !json {
            output::warn("Skipping tests (--force).");
        }
    }

    // Step 2: Push to main
    step_num += 1;
    if !json {
        output::step(step_num, total_steps, "Pushing to main...");
    }

    // Ensure we're on main
    Command::new("git")
        .args(["checkout", "main"])
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
        .args(["commit", "-m", "Deploy to production via spawn"])
        .current_dir(&project_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;

    cloud::github::push_branch(&project_dir, "main").await?;

    // Step 3: Vercel auto-deploys via GitHub integration, but we also trigger explicitly
    step_num += 1;
    if !json {
        output::step(
            step_num,
            total_steps,
            "Triggering Vercel production deployment...",
        );
    }
    let url = cloud::vercel::deploy_production(&project_dir).await?;

    if !json {
        println!();
        output::success(&format!(
            "Production deployed: {}",
            output::hyperlink(&url, &url)
        ));
        output::next_step("Your app is live. Run `spawn run claude` to keep building.");
    }

    let result = DeployOutput {
        production_url: Some(url),
        tests_passed,
        status: "deployed".to_string(),
        next_step: "spawn run claude".to_string(),
    };
    output::json_output(json, &result);

    Ok(())
}

/// Run the Playwright test suite inside the container.
async fn run_tests(config: &SpawnConfig) -> Result<bool> {
    let container = &config.container_name;

    if !docker::is_container_running(container).await? {
        docker::start_container(container).await?;
    }

    let ok = docker::exec_streaming(container, &["npm", "test"]).await?;
    Ok(ok)
}
