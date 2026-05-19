use anyhow::{Context, Result, bail};
use std::env;

use crate::cli::LogsArgs;
use crate::commands::api;
use crate::config::{LocalState, migrate_if_needed};
use crate::credentials::Credentials;
use crate::ui;

pub fn logs(args: LogsArgs, verbose: bool) -> Result<()> {
    let cwd = env::current_dir()?;
    migrate_if_needed(&cwd)?;
    let state = LocalState::load(&cwd).with_context(
        || "No .spx/state.json found. Run `spx new` or `spx run` from an SPX project first.",
    )?;
    let deployment_slug = state.deployment_slug.as_deref().ok_or_else(|| {
        anyhow::anyhow!("No deployment slug saved. Run `spx run FILENAME` first.")
    })?;

    if let Some(severity) = &args.severity {
        if severity != "info" && severity != "error" {
            bail!("--severity must be one of: info, error");
        }
    }

    let creds = Credentials::require()?;
    let api_url = api::api_url();
    let url = logs_url(&api_url, deployment_slug, &args);
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
        Err(ureq::Error::Status(_, resp)) => {
            let body = resp.into_string().unwrap_or_else(|_| {
                r#"{"error":{"code":"request_failed","message":"request failed with no body"}}"#
                    .into()
            });
            println!("{body}");
            std::process::exit(1);
        }
        Err(ureq::Error::Transport(t)) => bail!("GET {url} failed: {t}"),
    };

    let body = resp.into_string().context("reading response body")?;
    println!("{body}");
    Ok(())
}

fn logs_url(api_url: &str, deployment_slug: &str, args: &LogsArgs) -> String {
    let mut query = vec![format!("limit={}", args.limit)];
    if let Some(from) = &args.from {
        query.push(format!("from={}", percent_encode_query_value(from)));
    }
    if let Some(to) = &args.to {
        query.push(format!("to={}", percent_encode_query_value(to)));
    }
    if let Some(severity) = &args.severity {
        query.push(format!("severity={}", percent_encode_query_value(severity)));
    }
    format!(
        "{}/projects/{}/logs?{}",
        api_url.trim_end_matches('/'),
        percent_encode_query_value(deployment_slug),
        query.join("&")
    )
}

fn percent_encode_query_value(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_encode_query_value_encodes_iso_plus() {
        assert_eq!(
            percent_encode_query_value("2026-05-19T10:00:00+00:00"),
            "2026-05-19T10%3A00%3A00%2B00%3A00"
        );
    }

    #[test]
    fn logs_url_includes_filters() {
        let args = LogsArgs {
            from: Some("2026-05-19T10:00:00Z".into()),
            to: Some("2026-05-19T10:05:00Z".into()),
            limit: 25,
            severity: Some("error".into()),
        };
        assert_eq!(
            logs_url("https://api.example.test/", "quiet-fox", &args),
            "https://api.example.test/projects/quiet-fox/logs?limit=25&from=2026-05-19T10%3A00%3A00Z&to=2026-05-19T10%3A05%3A00Z&severity=error"
        );
    }
}
