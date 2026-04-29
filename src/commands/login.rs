use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::commands::api;
use crate::credentials::Credentials;
use crate::ui;

#[derive(Deserialize)]
struct DeviceAuthResponse {
    user_code: String,
    verification_uri: String,
    poll_token: String,
    interval: u64,
    expires_in: u64,
}

#[derive(Deserialize)]
struct TokenResponse {
    status: String,
    spx_token: Option<String>,
    username: Option<String>,
}

pub fn login(verbose: bool) -> Result<()> {
    let api_url = api::api_url();
    if verbose {
        ui::verbose(&format!("Control plane: {api_url}"));
    }

    // Step 1: POST /auth/device
    let device_url = format!("{}/auth/device", api_url.trim_end_matches('/'));
    if verbose {
        ui::verbose(&format!("POST {device_url}"));
    }

    let device_resp: DeviceAuthResponse = match ureq::post(&device_url)
        .send_json(serde_json::json!({}))
    {
        Ok(resp) => resp.into_json().context("parsing device auth response")?,
        Err(ureq::Error::Status(code, resp)) => {
            let body = resp.into_string().unwrap_or_else(|_| "<no body>".into());
            bail!("POST {device_url} returned {code}: {body}");
        }
        Err(ureq::Error::Transport(t)) => bail!("POST {device_url} failed: {t}"),
    };

    // Step 2: Show user code and open browser
    eprintln!();
    ui::info(&format!(
        "Enter this code at {}: {}",
        device_resp.verification_uri,
        device_resp.user_code.bold(),
    ));
    eprintln!();

    // Try to open the browser
    if let Err(e) = open::that(&device_resp.verification_uri) {
        if verbose {
            ui::verbose(&format!("Could not open browser: {e}"));
        }
        ui::info(&format!("Open {} in your browser", device_resp.verification_uri));
    }

    // Step 3: Poll /auth/token until ready or expired
    let token_url = format!("{}/auth/token", api_url.trim_end_matches('/'));
    let max_polls = device_resp.expires_in / device_resp.interval + 1;

    ui::info("Waiting for authorization...");

    for _ in 0..max_polls {
        std::thread::sleep(std::time::Duration::from_secs(device_resp.interval));

        if verbose {
            ui::verbose(&format!("POST {token_url}"));
        }

        let token_resp: TokenResponse = match ureq::post(&token_url)
            .send_json(serde_json::json!({ "poll_token": device_resp.poll_token }))
        {
            Ok(resp) => resp.into_json().context("parsing token response")?,
            Err(ureq::Error::Status(code, resp)) => {
                let body = resp.into_string().unwrap_or_else(|_| "<no body>".into());
                if verbose {
                    ui::verbose(&format!("POST {token_url} returned {code}: {body}"));
                }
                bail!("authentication failed ({code}): {body}");
            }
            Err(ureq::Error::Transport(t)) => bail!("POST {token_url} failed: {t}"),
        };

        match token_resp.status.as_str() {
            "pending" => {
                if verbose {
                    ui::verbose("Authorization pending...");
                }
                continue;
            }
            "ready" => {
                let token = token_resp.spx_token.context("server returned ready but no token")?;
                let username = token_resp.username.context("server returned ready but no username")?;

                let creds = Credentials {
                    username: username.clone(),
                    token,
                };
                creds.save()?;

                eprintln!();
                ui::success(&format!("Logged in as {username}"));
                return Ok(());
            }
            "expired" => {
                bail!("Device code expired. Run `spx login` to try again.");
            }
            other => {
                bail!("Unexpected status from server: {other}");
            }
        }
    }

    bail!("Timed out waiting for authorization. Run `spx login` to try again.")
}

pub fn login_with_code(code: &str, verbose: bool) -> Result<()> {
    let api_url = api::api_url();
    if verbose {
        ui::verbose(&format!("Control plane: {api_url}"));
    }

    let url = format!("{}/auth/code", api_url.trim_end_matches('/'));
    if verbose {
        ui::verbose(&format!("POST {url}"));
    }

    let resp: TokenResponse = match ureq::post(&url).send_json(serde_json::json!({ "code": code })) {
        Ok(r) => r.into_json().context("parsing /auth/code response")?,
        Err(ureq::Error::Status(401, _)) => bail!("Invalid registration code."),
        Err(ureq::Error::Status(503, _)) => {
            bail!("Code-based auth is not enabled on this control plane.")
        }
        Err(ureq::Error::Status(code_, r)) => {
            let body = r.into_string().unwrap_or_else(|_| "<no body>".into());
            bail!("POST {url} returned {code_}: {body}");
        }
        Err(ureq::Error::Transport(t)) => bail!("POST {url} failed: {t}"),
    };

    if resp.status != "ready" {
        bail!("Unexpected status from server: {}", resp.status);
    }

    let token = resp.spx_token.context("server returned ready but no token")?;
    let username = resp.username.context("server returned ready but no username")?;

    Credentials {
        username: username.clone(),
        token,
    }
    .save()?;

    eprintln!();
    ui::success(&format!("Logged in as {username}"));
    Ok(())
}

/// Helper trait just for bold formatting in this module.
trait Bold {
    fn bold(&self) -> String;
}

impl Bold for str {
    fn bold(&self) -> String {
        format!("\x1b[1m{self}\x1b[0m")
    }
}
