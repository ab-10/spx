use anyhow::{Context, Result, bail};
use std::io::{self, Write};

use crate::commands::api;
use crate::config::{self, LocalState};
use crate::credentials::Credentials;
use crate::ui;

const HACKATHON_API_URL: &str = "https://api.runspx.com";

pub fn hackathon_submit(verbose: bool) -> Result<()> {
    let creds = Credentials::require()?;
    let cwd = std::env::current_dir().context("determining current directory")?;
    config::migrate_if_needed(&cwd)?;
    let state = LocalState::load(&cwd).ok();

    println!("SPX Hackathon Submission");
    println!("Press Enter to accept a default shown in brackets.");
    println!();

    let default_name = state.as_ref().map(|s| s.project_name.as_str());
    let project_name = prompt_required("Project name", default_name)?;
    let default_url = state
        .as_ref()
        .and_then(|s| s.deployment_slug.as_ref())
        .map(|slug| format!("https://{slug}.runspx.com"));
    let project_url = prompt_required("Project URL", default_url.as_deref())?;
    let repository_url = prompt_optional("Repository URL (optional)", None)?;
    let description = prompt_multiline("Short description", 2000)?;

    if !project_url.starts_with("https://") {
        bail!("project URL must start with https://");
    }
    if let Some(url) = repository_url.as_ref() {
        if !url.starts_with("https://") {
            bail!("repository URL must start with https://");
        }
    }

    let api_url = HACKATHON_API_URL;
    println!("Submitting hackathon entry to {api_url}.");
    if verbose {
        ui::verbose(&format!(
            "POST {}/hackathon-submissions",
            api_url.trim_end_matches('/')
        ));
    }
    let payload = api::HackathonSubmissionRequest {
        project_name: &project_name,
        project_url: &project_url,
        repository_url: repository_url.as_deref(),
        description: &description,
    };
    let resp = api::post_hackathon_submission(&api_url, &creds.token, &payload)?;
    println!("Submitted {}: {}", resp.project_name, resp.project_url);
    Ok(())
}

fn prompt_required(label: &str, default: Option<&str>) -> Result<String> {
    loop {
        let value = prompt_line(label, default)?;
        if !value.trim().is_empty() {
            return Ok(value.trim().to_string());
        }
        println!("{label} is required.");
    }
}

fn prompt_optional(label: &str, default: Option<&str>) -> Result<Option<String>> {
    let value = prompt_line(label, default)?;
    let value = value.trim();
    if value.is_empty() {
        Ok(None)
    } else {
        Ok(Some(value.to_string()))
    }
}

fn prompt_line(label: &str, default: Option<&str>) -> Result<String> {
    match default {
        Some(default) => print!("{label} [{default}]: "),
        None => print!("{label}: "),
    }
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let value = line.trim_end_matches(['\r', '\n']);
    if value.is_empty() {
        Ok(default.unwrap_or_default().to_string())
    } else {
        Ok(value.to_string())
    }
}

fn prompt_multiline(label: &str, max_len: usize) -> Result<String> {
    println!("{label} (finish with an empty line):");
    let mut lines = Vec::new();
    loop {
        print!("> ");
        io::stdout().flush()?;
        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        lines.push(line.to_string());
    }
    let description = lines.join("\n").trim().to_string();
    if description.is_empty() {
        bail!("description is required");
    }
    if description.len() > max_len {
        bail!("description must be at most {max_len} characters");
    }
    Ok(description)
}
