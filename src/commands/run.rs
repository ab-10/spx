use anyhow::{bail, Context, Result};
use colored::Colorize;
use serde_json::Value;
use std::env;
use std::io::Write;
use std::path::Path;

use crate::cli::RunArgs;
use crate::commands::api;
use crate::credentials::Credentials;
use crate::ui;

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

    let api_url = api::api_url();
    if verbose {
        ui::verbose(&format!("Control plane: {api_url}"));
    }

    let archive = api::create_archive(&cwd)?;
    if verbose {
        ui::verbose(&format!("Archive size: {} bytes", archive.len()));
    }

    let resp = api::post_run(&api_url, &creds.token, &archive, &entry, verbose)?;

    eprintln!();
    eprintln!("  {}", ui::hyperlink(&resp.url, &resp.url));
    eprintln!(
        "  {} {}",
        "kill with:".dimmed(),
        format!("spx kill {}", resp.pet_name).dimmed()
    );
    eprintln!();

    let logs_url = format!(
        "{}/dproc/{}/logs?follow=true",
        api_url.trim_end_matches('/'),
        resp.pet_name
    );

    let mut exit_state: Option<(Option<i32>, Option<String>, String)> = None;
    api::stream_sse(&logs_url, &creds.token, 5, |ev| {
        match ev.event.as_str() {
            "log" => {
                let v: Value = serde_json::from_str(&ev.data).unwrap_or(Value::Null);
                let stream = v.get("stream").and_then(|s| s.as_str()).unwrap_or("stdout");
                let msg = v.get("msg").and_then(|s| s.as_str()).unwrap_or("");
                if stream == "stderr" {
                    let stderr = std::io::stderr();
                    let mut h = stderr.lock();
                    let _ = h.write_all(msg.as_bytes());
                    let _ = h.flush();
                } else {
                    let stdout = std::io::stdout();
                    let mut h = stdout.lock();
                    let _ = h.write_all(msg.as_bytes());
                    let _ = h.flush();
                }
                Ok(false)
            }
            "running" => Ok(false),
            "bind" => Ok(false),
            "exit" => {
                let v: Value = serde_json::from_str(&ev.data).unwrap_or(Value::Null);
                let code = v.get("code").and_then(|c| c.as_i64()).map(|c| c as i32);
                let signal = v
                    .get("signal")
                    .and_then(|s| s.as_str())
                    .map(|s| s.to_string());
                exit_state = Some((code, signal, "exited".into()));
                Ok(true)
            }
            "failed" => {
                let v: Value = serde_json::from_str(&ev.data).unwrap_or(Value::Null);
                let reason = v
                    .get("reason")
                    .and_then(|s| s.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                exit_state = Some((Some(1), None, format!("failed: {reason}")));
                Ok(true)
            }
            "gap" => {
                ui::warn("log stream gap (some events were buffered out)");
                Ok(false)
            }
            _ => Ok(false),
        }
    })?;

    match exit_state {
        Some((Some(code), _, _)) => std::process::exit(code),
        Some((None, Some(sig), _)) => {
            ui::warn(&format!("deproc terminated by signal {sig}"));
            std::process::exit(1);
        }
        Some((None, None, label)) => {
            ui::warn(&format!("deproc {label}"));
            std::process::exit(1);
        }
        None => Ok(()),
    }
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

    let rel = canonical
        .strip_prefix(&cwd_canonical)
        .map_err(|_| anyhow::anyhow!(
            "entry file must be inside the current directory (no ..-escape): {}",
            filename.display()
        ))?;

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
