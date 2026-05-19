use anyhow::{Context, Result, bail};
use colored::Colorize;
use std::collections::BTreeMap;
use std::env;
use std::path::Path;

use crate::cli::RunArgs;
use crate::commands::api;
use crate::config::{LocalState, migrate_if_needed, recover_state};
use crate::credentials::Credentials;
use crate::ui;

fn resolve_run_env_overrides(items: &[String]) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for item in items {
        if let Some((key, value)) = item.split_once('=') {
            if key.is_empty() {
                bail!("invalid --env value '{item}': missing key before '='");
            }
            out.insert(key.to_string(), value.to_string());
            continue;
        }
        let value = env::var(item).with_context(|| {
            format!(
                "--env {item} requires local process env var {item} to be set, or use --env {item}=VALUE"
            )
        })?;
        out.insert(item.to_string(), value);
    }
    Ok(out)
}

pub fn run(args: RunArgs, verbose: bool) -> Result<()> {
    let cwd = env::current_dir()?;

    if verbose {
        ui::verbose(&format!("Working directory: {}", cwd.display()));
    }

    let entry = resolve_entry(&cwd, &args.filename)?;
    if verbose {
        ui::verbose(&format!("Entry: {entry}"));
    }

    let creds = Credentials::require()?;
    migrate_if_needed(&cwd)?;
    let mut state = match LocalState::load(&cwd) {
        Ok(state) => state,
        Err(_) => recover_state(&cwd)?,
    };

    let api_url = api::api_url();
    let run_env_overrides = resolve_run_env_overrides(&args.env)?;
    if verbose {
        ui::verbose(&format!("Control plane: {api_url}"));
        ui::verbose(&format!("Project: {}", state.project_name));
    }

    let archive = api::create_archive(&cwd)?;
    if verbose {
        ui::verbose(&format!("Archive size: {} bytes", archive.len()));
    }

    let resp = api::post_run(
        &api_url,
        &creds.token,
        &archive,
        &entry,
        &state.project_name,
        state.deployment_slug.as_deref(),
        &run_env_overrides,
        verbose,
    )?;
    if resp.project_name != state.project_name {
        bail!(
            "server returned project '{}' for local project '{}'",
            resp.project_name,
            state.project_name
        );
    }
    if state.deployment_slug.as_deref() != Some(resp.deployment_slug.as_str()) {
        state.deployment_slug = Some(resp.deployment_slug.clone());
        state.save(&cwd)?;
    }

    eprintln!();
    eprintln!("  {}", ui::hyperlink(&resp.url, &resp.url));
    eprintln!(
        "  {} {}",
        "kill with:".dimmed(),
        format!("spx kill {}", resp.deployment_slug).dimmed()
    );
    eprintln!();

    return Ok(());

}

/// Resolve `filename` relative to `cwd`. Returns the relative path string
/// (forward-slash separated) suitable for sending on the wire.
///
/// Validates: file exists, is a regular file (not symlink), is under cwd
/// (no `..` escape), and ends in `.py`.
fn resolve_entry(cwd: &Path, filename: &Path) -> Result<String> {
    let candidate = if filename.is_absolute() {
        filename.to_path_buf()
    } else {
        cwd.join(filename)
    };

    let meta = std::fs::symlink_metadata(&candidate)
        .with_context(|| format!("file not found: {}", filename.display()))?;
    if meta.file_type().is_symlink() {
        bail!("entry file must not be a symlink: {}", filename.display());
    }
    if !meta.file_type().is_file() {
        bail!("entry path is not a regular file: {}", filename.display());
    }

    let canonical = candidate
        .canonicalize()
        .with_context(|| format!("canonicalizing {}", filename.display()))?;
    let cwd_canonical = cwd
        .canonicalize()
        .with_context(|| format!("canonicalizing cwd {}", cwd.display()))?;

    let rel = canonical.strip_prefix(&cwd_canonical).map_err(|_| {
        anyhow::anyhow!(
            "entry file must be inside the current directory (no ..-escape): {}",
            filename.display()
        )
    })?;

    if rel.extension().and_then(|s| s.to_str()) != Some("py") {
        bail!("entry file must end in .py: {}", filename.display());
    }

    let parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    Ok(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn resolve_entry_simple_file() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path();
        fs::write(cwd.join("hi.py"), "print('hi')").unwrap();
        let entry = resolve_entry(cwd, Path::new("hi.py")).unwrap();
        assert_eq!(entry, "hi.py");
    }

    #[test]
    fn resolve_entry_nested() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path();
        fs::create_dir_all(cwd.join("pkg/sub")).unwrap();
        fs::write(cwd.join("pkg/sub/main.py"), "print('hi')").unwrap();
        let entry = resolve_entry(cwd, Path::new("pkg/sub/main.py")).unwrap();
        assert_eq!(entry, "pkg/sub/main.py");
    }

    #[test]
    fn resolve_entry_dot_slash() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path();
        fs::write(cwd.join("hi.py"), "print('hi')").unwrap();
        let entry = resolve_entry(cwd, Path::new("./hi.py")).unwrap();
        assert_eq!(entry, "hi.py");
    }

    #[test]
    fn resolve_entry_rejects_non_py() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path();
        fs::write(cwd.join("hi.txt"), "hi").unwrap();
        assert!(resolve_entry(cwd, Path::new("hi.txt")).is_err());
    }

    #[test]
    fn resolve_entry_rejects_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path();
        assert!(resolve_entry(cwd, Path::new("nope.py")).is_err());
    }

    #[test]
    fn resolve_entry_rejects_escape() {
        let tmp = tempfile::tempdir().unwrap();
        let outer = tmp.path();
        fs::create_dir_all(outer.join("sub")).unwrap();
        fs::write(outer.join("evil.py"), "x").unwrap();
        let cwd = outer.join("sub");
        assert!(resolve_entry(&cwd, Path::new("../evil.py")).is_err());
    }

    #[test]
    fn resolve_entry_rejects_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path();
        fs::write(cwd.join("real.py"), "x").unwrap();
        std::os::unix::fs::symlink(cwd.join("real.py"), cwd.join("link.py")).unwrap();
        assert!(resolve_entry(cwd, Path::new("link.py")).is_err());
    }
}
