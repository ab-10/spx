use anyhow::{Context, Result};
use serde::Serialize;
use std::path::PathBuf;

use crate::config::SpawnConfig;
use crate::{cloud, docker, output};

#[derive(Serialize)]
struct InitOutput {
    project_name: String,
    project_dir: String,
    container_name: String,
    cloud_connected: bool,
    next_step: String,
}

pub async fn run(name: Option<String>, local: bool, json: bool) -> Result<()> {
    // Determine project name
    let project_name = match name {
        Some(n) => n,
        None => std::env::current_dir()?
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "my-app".to_string()),
    };

    let project_dir = std::env::current_dir()?.join(&project_name);
    let container_name = format!("spawn-{project_name}");

    if !json {
        output::header(&format!("Initializing spawn project: {project_name}"));
        if local {
            output::warn("Local mode — cloud wiring will be deferred to first deploy/preview.");
        }
    }

    let total_steps = if local { 4 } else { 7 };

    // Step 1: Check and pull Docker image
    if !json {
        output::step(1, total_steps, "Checking Docker...");
    }
    docker::check_docker().await?;
    docker::pull_image().await?;

    // Step 2: Create project directory and scaffold Next.js app
    if !json {
        output::step(
            2,
            total_steps,
            "Scaffolding Next.js app with TypeScript, Tailwind, App Router...",
        );
    }
    scaffold_nextjs(&project_name, &project_dir).await?;

    // Step 3: Create and start Docker container
    if !json {
        output::step(3, total_steps, "Creating Docker container...");
    }
    docker::create_container(
        &container_name,
        &project_dir.to_string_lossy(),
        docker::base_image(),
    )
    .await?;

    // Install dependencies inside container
    docker::exec_streaming(&container_name, &["npm", "install"])
        .await
        .context("Failed to install npm dependencies")?;

    // Step 4 (local): Set up Stack Auth with placeholders
    if local {
        if !json {
            output::step(4, total_steps, "Setting up Stack Auth (local placeholders)...");
        }
        cloud::stackauth::setup_local_placeholders(&project_dir)?;
    }

    let mut config = SpawnConfig {
        name: project_name.clone(),
        cloud_connected: false,
        container_name: container_name.clone(),
        docker_image: docker::base_image().to_string(),
        vercel: None,
        github: None,
        stack_auth: None,
    };

    if !local {
        // Step 3 (cloud): Provision Vercel Postgres
        if !json {
            output::step(3, total_steps, "Provisioning Vercel Postgres...");
        }
        let vercel_config =
            cloud::vercel::provision_project(&project_name, &project_dir).await?;
        config.vercel = Some(vercel_config);

        // Step 4 (cloud): Set up Stack Auth
        if !json {
            output::step(4, total_steps, "Setting up Stack Auth...");
        }
        let stack_config =
            cloud::stackauth::setup(&container_name, &project_dir).await?;
        config.stack_auth = Some(stack_config);

        // Step 5: Sync env vars to Vercel
        if !json {
            output::step(5, total_steps, "Syncing env vars to Vercel...");
        }
        cloud::vercel::sync_env_vars(&project_dir).await?;

        // Step 6: Create GitHub repo, push initial commit, link to Vercel
        if !json {
            output::step(6, total_steps, "Creating GitHub repo and linking to Vercel...");
        }
        let gh_config = cloud::github::create_repo(&project_name, &project_dir).await?;
        config.github = Some(gh_config);

        config.cloud_connected = true;
    }

    // Save config
    config.save(&project_dir)?;

    // Start the dev server in the background inside the container
    let _ = docker::exec_streaming(
        &container_name,
        &["sh", "-c", "npm run dev &"],
    )
    .await;

    if !json {
        output::success(&format!("Project '{project_name}' initialized successfully!"));
        output::success(&format!(
            "Container: {}",
            output::hyperlink(
                "http://localhost:3000",
                &format!("{container_name} → localhost:3000")
            )
        ));

        if local {
            output::next_step("Run `spawn run claude` to start an agentic coding session.");
        } else {
            output::next_step("Run `spawn run claude` to start building with your AI agent.");
        }
    }

    let result = InitOutput {
        project_name,
        project_dir: project_dir.to_string_lossy().to_string(),
        container_name: container_name.clone(),
        cloud_connected: config.cloud_connected,
        next_step: "spawn run claude".to_string(),
    };

    output::json_output(json, &result);

    // Drop into the container shell
    if !json {
        output::header("Dropping you into the container...");
        docker::shell(&container_name).await?;
    }

    Ok(())
}

/// Scaffold a Next.js app with TypeScript, Tailwind, and App Router.
async fn scaffold_nextjs(project_name: &str, project_dir: &PathBuf) -> Result<()> {
    if project_dir.exists() {
        // Check if it's already a Next.js project
        if project_dir.join("package.json").exists() {
            output::warn("Directory already exists with package.json. Skipping scaffold.");
            return Ok(());
        }
    }

    let status = tokio::process::Command::new("npx")
        .args([
            "create-next-app@14",
            project_name,
            "--typescript",
            "--tailwind",
            "--eslint",
            "--app",
            "--src-dir",
            "--import-alias",
            "@/*",
            "--use-npm",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .status()
        .await
        .context("Failed to run create-next-app. Is Node.js installed?")?;

    if !status.success() {
        anyhow::bail!("create-next-app failed. Check that npx and Node.js are available.");
    }

    // Create tests directory with a basic Playwright config
    let tests_dir = project_dir.join("tests");
    std::fs::create_dir_all(&tests_dir)?;

    std::fs::write(
        project_dir.join("playwright.config.ts"),
        r#"import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './tests',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: 'html',
  use: {
    baseURL: 'http://localhost:3000',
    trace: 'on-first-retry',
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
  webServer: {
    command: 'npm run dev',
    url: 'http://localhost:3000',
    reuseExistingServer: !process.env.CI,
  },
});
"#,
    )?;

    std::fs::write(
        tests_dir.join("app.spec.ts"),
        r#"import { test, expect } from '@playwright/test';

test('homepage loads', async ({ page }) => {
  await page.goto('/');
  await expect(page).toHaveTitle(/Next/);
});
"#,
    )?;

    // Add Playwright as a dev dependency (will be installed in container)
    let pkg_path = project_dir.join("package.json");
    if pkg_path.exists() {
        let contents = std::fs::read_to_string(&pkg_path)?;
        if let Ok(mut pkg) = serde_json::from_str::<serde_json::Value>(&contents) {
            if let Some(dev_deps) = pkg.get_mut("devDependencies") {
                if let Some(obj) = dev_deps.as_object_mut() {
                    obj.insert(
                        "@playwright/test".to_string(),
                        serde_json::Value::String("^1".to_string()),
                    );
                }
            }
            if let Some(scripts) = pkg.get_mut("scripts") {
                if let Some(obj) = scripts.as_object_mut() {
                    obj.insert(
                        "test".to_string(),
                        serde_json::Value::String("playwright test".to_string()),
                    );
                }
            }
            std::fs::write(&pkg_path, serde_json::to_string_pretty(&pkg)?)?;
        }
    }

    output::success("Next.js app scaffolded with TypeScript, Tailwind, and App Router.");
    Ok(())
}
