use anyhow::{bail, Context, Result};
use colored::Colorize;
use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::commands::api;
use crate::credentials::Credentials;
use crate::ui;

#[derive(Deserialize)]
struct DprocSummary {
    pet_name: String,
    state: String,
    started_at: f64,
    url: Option<String>,
}

pub fn ps(json: bool, verbose: bool) -> Result<()> {
    let creds = Credentials::require()?;
    let api_url = api::api_url();
    let url = format!("{}/dproc", api_url.trim_end_matches('/'));
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

    if json {
        println!("{body}");
        return Ok(());
    }

    let items: Vec<DprocSummary> =
        serde_json::from_str(&body).context("parsing /dproc response")?;

    if items.is_empty() {
        eprintln!("No running deployments. Start one with `spx run FILENAME`.");
        return Ok(());
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);

    let pet_w = items
        .iter()
        .map(|i| i.pet_name.len())
        .max()
        .unwrap_or(8)
        .max("PET NAME".len());
    let state_w = items
        .iter()
        .map(|i| i.state.len())
        .max()
        .unwrap_or(5)
        .max("STATE".len());
    let url_w = items
        .iter()
        .map(|i| i.url.as_deref().unwrap_or("-").len())
        .max()
        .unwrap_or(3)
        .max("URL".len());

    println!(
        "{:<pet_w$}  {:<state_w$}  {:<url_w$}  {}",
        "PET NAME".bold(),
        "STATE".bold(),
        "URL".bold(),
        "AGE".bold(),
        pet_w = pet_w,
        state_w = state_w,
        url_w = url_w,
    );

    for item in &items {
        let url_disp = item.url.as_deref().unwrap_or("-");
        let age = format_age((now - item.started_at).max(0.0));
        println!(
            "{:<pet_w$}  {:<state_w$}  {:<url_w$}  {}",
            item.pet_name,
            item.state,
            url_disp,
            age,
            pet_w = pet_w,
            state_w = state_w,
            url_w = url_w,
        );
    }

    Ok(())
}

fn format_age(secs: f64) -> String {
    let s = secs as u64;
    if s < 60 {
        format!("{s}s")
    } else if s < 3600 {
        format!("{}m{}s", s / 60, s % 60)
    } else if s < 86400 {
        format!("{}h{}m", s / 3600, (s % 3600) / 60)
    } else {
        format!("{}d{}h", s / 86400, (s % 86400) / 3600)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_age_seconds() {
        assert_eq!(format_age(0.0), "0s");
        assert_eq!(format_age(45.7), "45s");
    }

    #[test]
    fn format_age_minutes() {
        assert_eq!(format_age(60.0), "1m0s");
        assert_eq!(format_age(125.0), "2m5s");
    }

    #[test]
    fn format_age_hours() {
        assert_eq!(format_age(3600.0), "1h0m");
        assert_eq!(format_age(3725.0), "1h2m");
    }

    #[test]
    fn format_age_days() {
        assert_eq!(format_age(86400.0), "1d0h");
        assert_eq!(format_age(90000.0), "1d1h");
    }
}
