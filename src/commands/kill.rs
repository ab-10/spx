use anyhow::{bail, Result};

use crate::cli::KillArgs;
use crate::commands::api;
use crate::credentials::Credentials;
use crate::ui;

pub fn kill(args: KillArgs, verbose: bool) -> Result<()> {
    let creds = Credentials::require()?;
    let api_url = api::api_url();
    let url = format!(
        "{}/dproc/{}/kill",
        api_url.trim_end_matches('/'),
        args.pet_name
    );
    if verbose {
        ui::verbose(&format!("POST {url}"));
    }

    match ureq::post(&url)
        .set("Authorization", &format!("Bearer {}", creds.token))
        .call()
    {
        Ok(_) => {
            ui::success(&format!("Killed {}.", args.pet_name));
            Ok(())
        }
        Err(ureq::Error::Status(404, _)) => {
            bail!("no such running deproc: {}", args.pet_name)
        }
        Err(ureq::Error::Status(403, _)) => {
            bail!("not your deproc: {}", args.pet_name)
        }
        Err(ureq::Error::Status(401, _)) => {
            bail!("session invalid or expired. Run `spx login` to re-authenticate.")
        }
        Err(ureq::Error::Status(code, resp)) => {
            let body = resp.into_string().unwrap_or_else(|_| "<no body>".into());
            bail!("POST {url} returned {code}: {body}")
        }
        Err(ureq::Error::Transport(t)) => bail!("POST {url} failed: {t}"),
    }
}
