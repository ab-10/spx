use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::env;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::ui;

const DEFAULT_API_URL: &str = "https://api.runspx.com";
pub const PROVISION_POLL_INTERVALS: &[u64] = &[5, 5, 10, 10, 15, 15, 20, 20, 30, 30];

pub fn api_url() -> String {
    env::var("SPX_API_URL").unwrap_or_else(|_| DEFAULT_API_URL.to_string())
}

pub fn ensure_rclone_available() -> Result<()> {
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

pub fn rclone_sync(cwd: &Path, user: &str, verbose: bool) -> Result<()> {
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

#[derive(Deserialize)]
pub struct RunResponse {
    pub url: String,
    #[serde(default)]
    pub provisioning: bool,
}

pub fn post_run(api_url: &str, token: &str, verbose: bool) -> Result<RunResponse> {
    let url = format!("{}/run", api_url.trim_end_matches('/'));
    if verbose {
        ui::verbose(&format!("POST {url}"));
    }

    match ureq::post(&url)
        .set("Authorization", &format!("Bearer {token}"))
        .send_json(serde_json::json!({}))
    {
        Ok(resp) => {
            let run_resp: RunResponse = resp.into_json().context("parsing run response")?;
            Ok(run_resp)
        }
        Err(ureq::Error::Status(code, resp)) => {
            if code == 401 || code == 403 {
                bail!(
                    "session invalid or expired. Run `spx login` to re-authenticate."
                );
            }
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

pub fn poll_until_ready(api_url: &str, token: &str, verbose: bool) -> Result<String> {
    for delay in PROVISION_POLL_INTERVALS {
        if verbose {
            ui::verbose(&format!("Waiting {delay}s before retrying..."));
        }
        std::thread::sleep(std::time::Duration::from_secs(*delay));

        match post_run(api_url, token, verbose) {
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
pub fn parse_error_body(body: &str) -> Option<(String, Option<String>)> {
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
