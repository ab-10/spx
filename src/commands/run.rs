use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::cli::RunArgs;
use crate::config::{migrate_if_needed, recover_config, LocalState, SpxConfig};
use crate::ui;

const DEFAULT_API_URL: &str = "https://api.runspx.com";
const PROVISION_POLL_INTERVALS: &[u64] = &[5, 5, 10, 10, 15, 15, 20, 20, 30, 30];

pub fn run(args: RunArgs, verbose: bool) -> Result<()> {
    let cwd = env::current_dir()?;

    if verbose {
        ui::verbose(&format!("Working directory: {}", cwd.display()));
    }

    migrate_if_needed(&cwd)?;

    let config = if SpxConfig::exists(&cwd) {
        SpxConfig::load(&cwd)?
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

    // Resolve user BEFORE checking rclone so --user persists even if rclone
    // is not installed.
    let user = resolve_user(&args, &mut state, &cwd)?;
    if verbose {
        ui::verbose(&format!("User: {user}"));
    }

    ensure_rclone_available()?;

    let api_url = env::var("SPX_API_URL").unwrap_or_else(|_| DEFAULT_API_URL.to_string());
    if verbose {
        ui::verbose(&format!("Control plane: {api_url}"));
    }

    ui::step(1, 2, &format!("Syncing project to gs://spx-{user}/app/"));
    rclone_sync(&cwd, &user, verbose)?;
    ui::success("Sync complete.");

    ui::step(2, 2, "Requesting run on preview environment...");
    let resp = post_run(&api_url, &user, verbose)?;

    let url = if resp.provisioning {
        ui::info("First run — provisioning resources. This can take up to 5 minutes.");
        poll_until_ready(&api_url, &user, verbose)?
    } else {
        resp.url
    };

    ui::success("Run requested.");
    eprintln!();
    eprintln!("  {}", ui::hyperlink(&url, &url));

    Ok(())
}

fn resolve_user(args: &RunArgs, state: &mut LocalState, cwd: &Path) -> Result<String> {
    if let Some(name) = &args.user {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            bail!("--user cannot be empty");
        }
        state.user = Some(trimmed.to_string());
        state.save(cwd)?;
        return Ok(trimmed.to_string());
    }

    if let Some(name) = &state.user {
        return Ok(name.clone());
    }

    bail!(
        "No user set for this project. Run `spx run --user <name>` to set your identity."
    )
}

fn ensure_rclone_available() -> Result<()> {
    let output = Command::new("rclone")
        .arg("version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output();

    match output {
        Ok(o) if o.status.success() => Ok(()),
        _ => bail!(
            "rclone is not available on PATH. Install with `brew install rclone` \
             or see https://rclone.org/install/ for other platforms."
        ),
    }
}

fn rclone_sync(cwd: &Path, user: &str, verbose: bool) -> Result<()> {
    let bucket = format!("gs://spx-{user}/app/");
    let cmd_str = format!(
        "rclone sync . {bucket} --checksum --exclude .git/** --exclude __pycache__/** \
         --exclude .venv/** --exclude .spx/**"
    );
    if verbose {
        ui::stream_header(&cmd_str);
    }

    let status = Command::new("rclone")
        .current_dir(cwd)
        .args([
            "sync",
            ".",
            &bucket,
            "--checksum",
            "--exclude",
            ".git/**",
            "--exclude",
            "__pycache__/**",
            "--exclude",
            ".venv/**",
            "--exclude",
            ".spx/**",
        ])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("failed to spawn rclone")?;

    if !status.success() {
        let code = status.code().unwrap_or(-1);
        bail!("rclone exited with status {code}");
    }

    Ok(())
}

#[derive(Serialize)]
struct RunRequest<'a> {
    user: &'a str,
}

#[derive(Deserialize)]
struct RunResponse {
    url: String,
    #[serde(default)]
    provisioning: bool,
}

fn post_run(api_url: &str, user: &str, verbose: bool) -> Result<RunResponse> {
    let url = format!("{}/run", api_url.trim_end_matches('/'));
    let body = RunRequest { user };
    let payload = serde_json::to_value(&body).context("serializing run request")?;
    if verbose {
        ui::verbose(&format!("POST {url}"));
        ui::verbose(&format!("body: {payload}"));
    }

    match ureq::post(&url).send_json(payload) {
        Ok(resp) => {
            let run_resp: RunResponse = resp.into_json().context("parsing run response")?;
            Ok(run_resp)
        }
        Err(ureq::Error::Status(code, resp)) => {
            let body = resp.into_string().unwrap_or_else(|_| "<no body>".into());
            if let Some((summary, output)) = parse_error_body(&body) {
                ui::warn(&summary);
                if let Some(output) = output {
                    eprintln!("{output}");
                }
                std::process::exit(1);
            }
            bail!("POST {url} returned {code}: {body}");
        }
        Err(ureq::Error::Transport(t)) => bail!("POST {url} failed: {t}"),
    }
}

fn poll_until_ready(api_url: &str, user: &str, verbose: bool) -> Result<String> {
    for delay in PROVISION_POLL_INTERVALS {
        if verbose {
            ui::verbose(&format!("Waiting {delay}s before retrying..."));
        }
        std::thread::sleep(std::time::Duration::from_secs(*delay));

        match post_run(api_url, user, verbose) {
            Ok(resp) if !resp.provisioning => return Ok(resp.url),
            Ok(_) => {
                if verbose {
                    ui::verbose("Still provisioning...");
                }
            }
            Err(e) => {
                if verbose {
                    ui::verbose(&format!("Retry failed: {e}"));
                }
            }
        }
    }
    bail!("Timed out waiting for environment to become ready. Try running `spx run` again.")
}

/// Try to extract a readable error from the nested JSON error response.
/// Returns (summary, optional process output), or None if parsing fails.
fn parse_error_body(body: &str) -> Option<(String, Option<String>)> {
    let outer: serde_json::Value = serde_json::from_str(body).ok()?;
    let detail = outer.get("detail")?.as_str()?;

    // detail looks like: "sidecar restart failed (500): {\"error\": \"...\"}"
    let json_start = detail.find('{')?;
    let inner: serde_json::Value = serde_json::from_str(&detail[json_start..]).ok()?;
    let error = inner.get("error")?.as_str()?;

    if let Some(idx) = error.find("\n--- process output ---\n") {
        let summary = error[..idx].to_string();
        let output = error[idx + 1..].to_string(); // keep the "--- process output ---" line
        Some((summary, Some(output)))
    } else {
        Some((error.to_string(), None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_error_body_with_process_output() {
        let body = r#"{"detail":"sidecar restart failed (500): {\"error\": \"user process did not bind 127.0.0.1:8001 within 60.0s\\n--- process output ---\\nwarning: bad thing\\nerror: Failed to spawn: `main.py`\"}"}"#;
        let (summary, output) = parse_error_body(body).unwrap();
        assert_eq!(summary, "user process did not bind 127.0.0.1:8001 within 60.0s");
        let output = output.unwrap();
        assert!(output.starts_with("--- process output ---\n"));
        assert!(output.contains("Failed to spawn"));
    }

    #[test]
    fn parse_error_body_without_process_output() {
        let body = r#"{"detail":"sidecar restart failed (500): {\"error\": \"something went wrong\"}"}"#;
        let (summary, output) = parse_error_body(body).unwrap();
        assert_eq!(summary, "something went wrong");
        assert!(output.is_none());
    }

    #[test]
    fn parse_error_body_invalid_json() {
        assert!(parse_error_body("not json").is_none());
    }

    #[test]
    fn parse_error_body_no_detail() {
        assert!(parse_error_body(r#"{"other": "field"}"#).is_none());
    }

    #[test]
    fn parse_error_body_no_inner_json() {
        let body = r#"{"detail":"plain text error without json"}"#;
        assert!(parse_error_body(body).is_none());
    }
}
