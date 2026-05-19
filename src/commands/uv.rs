use anyhow::{Context, Result};
use std::env;

use crate::cli::{UvAddArgs, UvArgs, UvCommand, UvRemoveArgs};
use crate::commands::api;
use crate::config::{LocalState, migrate_if_needed};
use crate::credentials::Credentials;

pub fn uv(args: UvArgs, verbose: bool) -> Result<()> {
    match args.command {
        UvCommand::Add(add_args) => uv_add(add_args, verbose),
        UvCommand::Remove(remove_args) => uv_remove(remove_args, verbose),
        UvCommand::List => uv_list(verbose),
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

fn dep_name_from_requirement(requirement: &str) -> Result<String> {
    let trimmed = requirement.trim();
    if trimmed.is_empty() {
        anyhow::bail!("requirement must not be empty");
    }
    let mut name = String::new();
    for ch in trimmed.chars() {
        if ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-' {
            name.push(ch);
        } else {
            break;
        }
    }
    if name.is_empty() {
        anyhow::bail!("requirement must start with a package name");
    }
    Ok(name.to_ascii_lowercase().replace('_', "-"))
}

fn uv_add(args: UvAddArgs, verbose: bool) -> Result<()> {
    let creds = Credentials::require()?;
    let api_url = api::api_url();
    let deployment_slug = project_deployment_slug()?;
    let dep_name = dep_name_from_requirement(&args.requirement)?;
    if verbose {
        eprintln!("[verbose] PUT {api_url}/projects/{deployment_slug}/deps/{dep_name}");
    }
    api::dep_set(
        &api_url,
        &creds.token,
        &deployment_slug,
        &dep_name,
        args.requirement.trim(),
    )?;
    println!("added {dep_name} to deployment dependencies");
    println!("run `spx run <file>` to deploy with updated dependencies");
    Ok(())
}

fn uv_remove(args: UvRemoveArgs, verbose: bool) -> Result<()> {
    let creds = Credentials::require()?;
    let api_url = api::api_url();
    let deployment_slug = project_deployment_slug()?;
    let dep_name = args.name.trim().to_ascii_lowercase().replace('_', "-");
    if verbose {
        eprintln!("[verbose] DELETE {api_url}/projects/{deployment_slug}/deps/{dep_name}");
    }
    api::dep_unset(&api_url, &creds.token, &deployment_slug, &dep_name)?;
    println!("removed {dep_name} from deployment dependencies");
    Ok(())
}

fn uv_list(verbose: bool) -> Result<()> {
    let creds = Credentials::require()?;
    let api_url = api::api_url();
    let deployment_slug = project_deployment_slug()?;
    if verbose {
        eprintln!("[verbose] GET {api_url}/projects/{deployment_slug}/deps");
    }
    let resp = api::dep_list(&api_url, &creds.token, &deployment_slug)?;
    if resp.dependencies.is_empty() {
        println!("no deployment dependencies configured");
        return Ok(());
    }
    println!("deployment dependencies for {} ({})", resp.project_name, resp.deployment_slug);
    for dep in resp.dependencies {
        match dep.updated_at {
            Some(ts) => println!("{}\t{}\t{}", dep.name, dep.requirement, ts),
            None => println!("{}\t{}", dep.name, dep.requirement),
        }
    }
    Ok(())
}
