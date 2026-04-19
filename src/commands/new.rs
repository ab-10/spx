use anyhow::{bail, Context, Result};
use std::env;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::cli::NewArgs;
use crate::commands::api;
use crate::config::LocalState;
use crate::credentials::Credentials;
use crate::ui;

pub fn new_project(args: NewArgs, verbose: bool) -> Result<()> {
    let name = &args.name;

    validate_name(name)?;

    let creds = Credentials::require()?;
    let user = &creds.username;

    let cwd = env::current_dir()?;
    let project_dir = cwd.join(name);

    if project_dir.exists() {
        bail!("Directory '{}' already exists", name);
    }

    let total_steps = 6;

    // 1. Create project directory and write scaffolding files
    ui::step(1, total_steps, "Scaffolding project...");
    std::fs::create_dir(&project_dir)
        .with_context(|| format!("creating directory '{name}'"))?;

    write_pyproject_toml(&project_dir, name)?;
    write_main_py(&project_dir, name)?;
    write_gitignore(&project_dir)?;

    let state = LocalState::init(name);
    state.save(&project_dir)?;

    ui::success("Project files created.");

    // 2. git init
    ui::step(2, total_steps, "Initializing git repository...");
    run_command("git", &["init"], &project_dir, verbose)?;
    ui::success("Git initialized.");

    // 3. uv sync
    ui::step(3, total_steps, "Installing dependencies (uv sync)...");
    run_command("uv", &["sync"], &project_dir, verbose)?;
    ui::success("Dependencies installed.");

    // 4. Check rclone
    ui::step(4, total_steps, "Checking rclone...");
    api::ensure_rclone_available()?;
    ui::success("rclone available.");

    // 5. rclone sync
    ui::step(5, total_steps, &format!("Syncing project to gs://spx-{user}/app/"));
    api::rclone_sync(&project_dir, user, verbose)?;
    ui::success("Sync complete.");

    // 6. POST /run
    ui::step(6, total_steps, "Requesting run on preview environment...");
    let api_url = api::api_url();
    if verbose {
        ui::verbose(&format!("Control plane: {api_url}"));
    }

    let resp = api::post_run(&api_url, &creds.token, verbose)?;

    let url = if resp.provisioning {
        ui::info("First run — provisioning resources. This can take up to 5 minutes.");
        api::poll_until_ready(&api_url, &creds.token, verbose)?
    } else {
        resp.url
    };

    ui::success("Run requested.");
    eprintln!();
    eprintln!("  {}", ui::hyperlink(&url, &url));
    eprintln!();
    ui::info(&format!("cd {name} to start working on your project."));

    Ok(())
}

/// Validate project name: lowercase alphanumeric and hyphens, must start with a letter.
fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("Project name cannot be empty");
    }
    if !name.chars().next().unwrap().is_ascii_lowercase() {
        bail!("Project name must start with a lowercase letter");
    }
    if !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        bail!("Project name must contain only lowercase letters, digits, and hyphens");
    }
    if name.ends_with('-') {
        bail!("Project name must not end with a hyphen");
    }
    Ok(())
}

fn run_command(program: &str, args: &[&str], cwd: &Path, verbose: bool) -> Result<()> {
    if verbose {
        ui::stream_header(&format!("{} {}", program, args.join(" ")));
    }

    let status = Command::new(program)
        .current_dir(cwd)
        .args(args)
        .stdout(if verbose { Stdio::inherit() } else { Stdio::null() })
        .stderr(if verbose { Stdio::inherit() } else { Stdio::null() })
        .status()
        .with_context(|| format!("failed to spawn {program}"))?;

    if !status.success() {
        let code = status.code().unwrap_or(-1);
        bail!("{program} exited with status {code}");
    }

    Ok(())
}

fn write_pyproject_toml(dir: &Path, name: &str) -> Result<()> {
    let content = format!(
        r#"[project]
name = "{name}"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = [
    "fastapi>=0.115",
    "uvicorn[standard]>=0.34",
]
"#
    );
    std::fs::write(dir.join("pyproject.toml"), content).context("writing pyproject.toml")
}

fn write_main_py(dir: &Path, name: &str) -> Result<()> {
    let content = format!(
        r#"import os
from fastapi import FastAPI

app = FastAPI()


@app.get("/")
def root():
    return {{"message": "hello from {name}"}}


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=int(os.environ.get("PORT", "8000")))
"#
    );
    std::fs::write(dir.join("main.py"), content).context("writing main.py")
}

fn write_gitignore(dir: &Path) -> Result<()> {
    let content = ".venv/\n__pycache__/\n.spx/\n";
    std::fs::write(dir.join(".gitignore"), content).context("writing .gitignore")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_name_valid() {
        assert!(validate_name("my-app").is_ok());
        assert!(validate_name("app123").is_ok());
        assert!(validate_name("a").is_ok());
    }

    #[test]
    fn validate_name_empty() {
        assert!(validate_name("").is_err());
    }

    #[test]
    fn validate_name_starts_with_digit() {
        assert!(validate_name("123app").is_err());
    }

    #[test]
    fn validate_name_uppercase() {
        assert!(validate_name("MyApp").is_err());
    }

    #[test]
    fn validate_name_special_chars() {
        assert!(validate_name("my_app").is_err());
        assert!(validate_name("my.app").is_err());
    }

    #[test]
    fn validate_name_trailing_hyphen() {
        assert!(validate_name("my-app-").is_err());
    }
}
