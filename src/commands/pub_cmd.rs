use anyhow::{Context, Result, bail};
use std::path::Path;

use crate::cli::{PubCreateArgs, PubDeleteArgs, PubUpdateArgs};
use crate::commands::{api, subscribe};
use crate::credentials::Credentials;
use crate::ui;

const MAX_PUB_BYTES: u64 = 10 * 1024 * 1024;

fn read_html_file(path: &Path) -> Result<(Vec<u8>, String)> {
    let meta = std::fs::metadata(path)
        .with_context(|| format!("reading metadata for {}", path.display()))?;
    if !meta.is_file() {
        bail!("{} is not a file", path.display());
    }
    if meta.len() > MAX_PUB_BYTES {
        bail!("{} exceeds the 10 MB pub upload limit", path.display());
    }
    let filename = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("{} has no valid filename", path.display()))?
        .to_string();
    let lower = filename.to_ascii_lowercase();
    if !lower.ends_with(".html") && !lower.ends_with(".htm") {
        bail!("pub file must end in .html or .htm");
    }
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    Ok((bytes, filename))
}

fn auto_subscribe_requested(args: &PubCreateArgs) -> bool {
    args.subscribe
        || std::env::var("SPX_AUTO_SUBSCRIBE")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
}

pub fn create(args: PubCreateArgs, verbose: bool) -> Result<()> {
    let creds = Credentials::require()?;
    let api_url = api::api_url();
    let (bytes, filename) = read_html_file(&args.path)?;
    if verbose {
        ui::verbose(&format!("POST {}/pub", api_url.trim_end_matches('/')));
        ui::verbose(&format!("File size: {} bytes", bytes.len()));
    }

    let resp = match api::pub_create(&api_url, &creds.token, &bytes, &filename) {
        Ok(resp) => resp,
        Err(err) => {
            let Some(payment) = err.downcast_ref::<api::PaymentRequired>() else {
                return Err(err);
            };
            // Never prompt: agents and CI run this non-interactively. Either the
            // caller opted in to checkout up front, or we print the next command.
            if !auto_subscribe_requested(&args) {
                ui::warn(&payment.to_string());
                bail!(
                    "publishing requires an active subscription. Run `spx subscribe`, \
                     then publish again (or re-run with `--subscribe`)."
                );
            }
            ui::info("Subscription required — starting checkout.");
            subscribe::subscribe(verbose)?;
            api::pub_create(&api_url, &creds.token, &bytes, &filename)?
        }
    };

    println!("{}", resp.url);
    Ok(())
}

pub fn update(args: PubUpdateArgs, verbose: bool) -> Result<()> {
    let creds = Credentials::require()?;
    let api_url = api::api_url();
    let (bytes, filename) = read_html_file(&args.path)?;
    if verbose {
        ui::verbose(&format!(
            "PUT {}/pub/{}",
            api_url.trim_end_matches('/'),
            args.slug_or_url
        ));
        ui::verbose(&format!("File size: {} bytes", bytes.len()));
    }
    let resp = api::pub_update(&api_url, &creds.token, &args.slug_or_url, &bytes, &filename)?;
    println!("{}", resp.url);
    Ok(())
}

pub fn delete(args: PubDeleteArgs, verbose: bool) -> Result<()> {
    let creds = Credentials::require()?;
    let api_url = api::api_url();
    if verbose {
        ui::verbose(&format!(
            "DELETE {}/pub/{}",
            api_url.trim_end_matches('/'),
            args.slug_or_url
        ));
    }
    api::pub_delete(&api_url, &creds.token, &args.slug_or_url)?;
    println!("deleted {}", args.slug_or_url);
    Ok(())
}

pub fn list(verbose: bool) -> Result<()> {
    let creds = Credentials::require()?;
    let api_url = api::api_url();
    if verbose {
        ui::verbose(&format!("GET {}/pub", api_url.trim_end_matches('/')));
    }
    let resp = api::pub_list(&api_url, &creds.token)?;
    if resp.pubs.is_empty() {
        println!("no pubs");
        return Ok(());
    }
    for pub_item in resp.pubs {
        println!("{}", pub_item.url);
    }
    Ok(())
}
