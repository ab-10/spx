use anyhow::{Context, Result, bail};

use crate::commands::api;
use crate::credentials::Credentials;
use crate::ui;

pub fn ps(verbose: bool) -> Result<()> {
    let creds = Credentials::require()?;
    let api_url = api::api_url();
    let url = format!("{}/projects", api_url.trim_end_matches('/'));
    if verbose {
        ui::verbose(&format!("GET {url}"));
    }

    let resp = match ureq::get(&url)
        .set("Authorization", &format!("Bearer {}", creds.token))
        .call()
    {
        Ok(r) => r,
        Err(ureq::Error::Status(401, _)) => {
            bail!("session invalid or expired. Run `spx login` to re-authenticate.")
        }
        Err(ureq::Error::Status(code, resp)) => {
            let body = resp.into_string().unwrap_or_else(|_| "<no body>".into());
            bail!("GET {url} returned {code}: {body}")
        }
        Err(ureq::Error::Transport(t)) => bail!("GET {url} failed: {t}"),
    };

    let body = resp.into_string().context("reading response body")?;
    println!("{body}");
    Ok(())
}
