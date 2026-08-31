use anyhow::{Context, Result, bail};
use std::path::Path;

use crate::cli::{PubCreateArgs, PubDeleteArgs, PubUpdateArgs};
use crate::commands::api;
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

pub fn create(args: PubCreateArgs, verbose: bool) -> Result<()> {
    let creds = Credentials::require()?;
    let api_url = api::api_url();
    let (bytes, filename) = read_html_file(&args.path)?;
    if verbose {
        ui::verbose(&format!("POST {}/pub", api_url.trim_end_matches('/')));
        ui::verbose(&format!("File size: {} bytes", bytes.len()));
    }
    let resp = api::pub_create(&api_url, &creds.token, &bytes, &filename)?;
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
