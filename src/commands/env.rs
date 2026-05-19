use anyhow::{Context, Result, bail};
use std::env;
use std::io::Read;

use crate::cli::{EnvArgs, EnvCommand, EnvLoadArgs, EnvSetArgs, EnvUnsetArgs};
use crate::commands::api;
use crate::config::{LocalState, migrate_if_needed};
use crate::credentials::Credentials;

pub fn env(args: EnvArgs, verbose: bool) -> Result<()> {
    match args.command {
        EnvCommand::Set(set_args) => env_set(set_args, verbose),
        EnvCommand::Unset(unset_args) => env_unset(unset_args, verbose),
        EnvCommand::List => env_list(verbose),
        EnvCommand::Load(load_args) => env_load(load_args, verbose),
    }
}

fn project_deployment_slug() -> Result<String> {
    let cwd = env::current_dir()?;
    migrate_if_needed(&cwd)?;
    let state = LocalState::load(&cwd).with_context(
        || "No .spx/state.json found. Run `spx new` or `spx run` from an SPX project first.",
    )?;
    state
        .deployment_slug
        .ok_or_else(|| anyhow::anyhow!("No deployment slug saved. Run `spx run FILENAME` first."))
}

fn split_key_pair(input: &str) -> (String, Option<String>) {
    if let Some((k, v)) = input.split_once('=') {
        (k.to_string(), Some(v.to_string()))
    } else {
        (input.to_string(), None)
    }
}

fn resolve_set_value(args: &EnvSetArgs) -> Result<(String, String)> {
    let (key, inline_value) = split_key_pair(&args.key_or_pair);
    if key.is_empty() {
        bail!("env key must not be empty");
    }
    let sources = (inline_value.is_some() as u8) + (args.from_stdin as u8) + (args.from_env as u8);
    if sources > 1 {
        bail!("choose exactly one value source: KEY=value, --from-stdin, or --from-env");
    }
    if sources == 0 {
        bail!(
            "missing value for {key}\n\nUse one of:\n  spx env set {key}=value\n  spx env set {key} --from-stdin\n  spx env set {key} --from-env"
        );
    }

    let value = if let Some(v) = inline_value {
        v
    } else if args.from_stdin {
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input)?;
        input.trim_end_matches(['\r', '\n']).to_string()
    } else {
        env::var(&key).with_context(|| format!("local environment variable {key} is not set"))?
    };

    Ok((key, value))
}

fn env_set(args: EnvSetArgs, verbose: bool) -> Result<()> {
    let creds = Credentials::require()?;
    let api_url = api::api_url();
    let deployment_slug = project_deployment_slug()?;
    let (key, value) = resolve_set_value(&args)?;
    if verbose {
        eprintln!("[verbose] PUT {api_url}/projects/{deployment_slug}/env/{key}");
    }
    api::env_set(&api_url, &creds.token, &deployment_slug, &key, &value)?;
    println!("saved {key} for this project");
    Ok(())
}

fn env_unset(args: EnvUnsetArgs, verbose: bool) -> Result<()> {
    let creds = Credentials::require()?;
    let api_url = api::api_url();
    let deployment_slug = project_deployment_slug()?;
    if verbose {
        eprintln!(
            "[verbose] DELETE {api_url}/projects/{deployment_slug}/env/{}",
            args.key
        );
    }
    api::env_unset(&api_url, &creds.token, &deployment_slug, &args.key)?;
    println!("removed {} from this project", args.key);
    Ok(())
}

fn env_list(verbose: bool) -> Result<()> {
    let creds = Credentials::require()?;
    let api_url = api::api_url();
    let deployment_slug = project_deployment_slug()?;
    if verbose {
        eprintln!("[verbose] GET {api_url}/projects/{deployment_slug}/env");
    }
    let resp = api::env_list(&api_url, &creds.token, &deployment_slug)?;
    if resp.variables.is_empty() {
        println!("no persisted env variables for this project");
        return Ok(());
    }
    println!("project: {} ({})", resp.project_name, resp.deployment_slug);
    for var in resp.variables {
        match var.updated_at {
            Some(ts) => println!("{}\t{}", var.key, ts),
            None => println!("{}", var.key),
        }
    }
    Ok(())
}

fn parse_env_file(contents: &str) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    for (idx, raw_line) in contents.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line.split_once('=').ok_or_else(|| {
            anyhow::anyhow!("invalid env file line {}: expected KEY=value", idx + 1)
        })?;
        let key = key.trim();
        if key.is_empty() {
            bail!("invalid env file line {}: empty key", idx + 1);
        }
        out.push((key.to_string(), value.to_string()));
    }
    Ok(out)
}

fn env_load(args: EnvLoadArgs, verbose: bool) -> Result<()> {
    let creds = Credentials::require()?;
    let api_url = api::api_url();
    let deployment_slug = project_deployment_slug()?;
    let file_text = std::fs::read_to_string(&args.file)
        .with_context(|| format!("reading {}", args.file.display()))?;
    let entries = parse_env_file(&file_text)?;
    for (key, value) in &entries {
        if verbose {
            eprintln!("[verbose] PUT {api_url}/projects/{deployment_slug}/env/{key}");
        }
        api::env_set(&api_url, &creds.token, &deployment_slug, key, value)?;
    }
    println!(
        "loaded {} variables from {}",
        entries.len(),
        args.file.display()
    );
    Ok(())
}
