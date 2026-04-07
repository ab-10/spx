use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::env;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::cli::RunArgs;
use crate::config::{migrate_if_needed, recover_config, LocalState, SpxConfig};
use crate::ui;

const DEFAULT_API_URL: &str = "https://spx-api.runspx.com";

pub fn run(args: RunArgs, verbose: bool) -> Result<()> {
    let cwd = env::current_dir()?;

    if verbose {
        ui::verbose(&format!("Working directory: {}", cwd.display()));
    }

    migrate_if_needed(&cwd)?;

    let config = if SpxConfig::exists(&cwd) {
        SpxConfig::load(&cwd)?
    } else {
        recover_config(&cwd)?
    };

    let mut state = if LocalState::exists(&cwd) {
        LocalState::load(&cwd)?
    } else {
        let s = LocalState::init(&config.project_name);
        s.save(&cwd)?;
        s
    };

    // Resolve user BEFORE checking rclone so --user persists even if rclone
    // is not installed.
    let user = resolve_user(&args, &mut state, &cwd)?;
    if verbose {
        ui::verbose(&format!("User: {user}"));
    }

    ensure_rclone_available()?;

    let api_url = env::var("SPX_API_URL").unwrap_or_else(|_| DEFAULT_API_URL.to_string());
    if verbose {
        ui::verbose(&format!("Control plane: {api_url}"));
    }

    ui::step(1, 2, &format!("Syncing project to gs://spx-{user}/app/"));
    let (changed, deleted) = rclone_sync_and_parse(&cwd, &user, verbose)?;
    ui::success(&format!(
        "Sync complete — {} changed, {} deleted",
        changed.len(),
        deleted.len()
    ));

    ui::step(2, 2, "Requesting run on preview environment...");
    post_run(&api_url, &user, &changed, &deleted, verbose)?;
    ui::success("Run requested.");

    ui::next_step("Watch your preview environment logs to see the restart.");

    Ok(())
}

fn resolve_user(args: &RunArgs, state: &mut LocalState, cwd: &Path) -> Result<String> {
    if let Some(name) = &args.user {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            bail!("--user cannot be empty");
        }
        state.user = Some(trimmed.to_string());
        state.save(cwd)?;
        return Ok(trimmed.to_string());
    }

    if let Some(name) = &state.user {
        return Ok(name.clone());
    }

    bail!(
        "No user set for this project. Run `spx run --user <name>` to set your identity."
    )
}

fn ensure_rclone_available() -> Result<()> {
    let output = Command::new("rclone")
        .arg("version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output();

    match output {
        Ok(o) if o.status.success() => Ok(()),
        _ => bail!(
            "rclone is not available on PATH. Install with `brew install rclone` \
             or see https://rclone.org/install/ for other platforms."
        ),
    }
}

fn rclone_sync_and_parse(
    cwd: &Path,
    user: &str,
    verbose: bool,
) -> Result<(Vec<String>, Vec<String>)> {
    let bucket = format!("gs://spx-{user}/app/");
    let cmd_str = format!(
        "rclone sync . {bucket} --checksum --exclude .git/** --exclude __pycache__/** \
         --exclude .venv/** --exclude .spx/** --log-level INFO"
    );
    ui::stream_header(&cmd_str);

    let mut child = Command::new("rclone")
        .current_dir(cwd)
        .args([
            "sync",
            ".",
            &bucket,
            "--checksum",
            "--exclude",
            ".git/**",
            "--exclude",
            "__pycache__/**",
            "--exclude",
            ".venv/**",
            "--exclude",
            ".spx/**",
            "--log-level",
            "INFO",
        ])
        .stdout(Stdio::inherit())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to spawn rclone")?;

    let stderr = child.stderr.take().expect("stderr captured");
    let reader = BufReader::new(stderr);

    let mut changed: Vec<String> = Vec::new();
    let mut deleted: Vec<String> = Vec::new();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let line = line.trim_end_matches('\r').to_string();
        eprintln!("{line}");
        if let Some(event) = parse_rclone_line(&line) {
            if verbose {
                ui::verbose(&format!("parsed: {event:?}"));
            }
            match event {
                RcloneEvent::Copied(p) => changed.push(format!("app/{p}")),
                RcloneEvent::Deleted(p) => deleted.push(p),
            }
        }
    }

    let status = child.wait().context("waiting for rclone")?;
    if !status.success() {
        let code = status.code().unwrap_or(-1);
        bail!("rclone exited with status {code}");
    }

    Ok((changed, deleted))
}

#[derive(Debug, PartialEq, Eq)]
enum RcloneEvent {
    Copied(String),
    Deleted(String),
}

/// Parse a single rclone INFO log line. Expected shape:
///   `2026/04/05 08:48:06 INFO  : path/to/file: Copied (new)`
fn parse_rclone_line(line: &str) -> Option<RcloneEvent> {
    // Locate " INFO" so we can discard everything up to and including the
    // log-level token and whitespace padding.
    let after_info = line.split(" INFO").nth(1)?;
    let after_info = after_info.trim_start();

    // The next character is typically ':' separator after padding.
    let rest = after_info.strip_prefix(':').unwrap_or(after_info);
    let rest = rest.trim();

    // `rest` should now look like "<path>: <message>" — split from the right
    // so that paths containing ':' or spaces are preserved.
    let (path, message) = rest.rsplit_once(": ")?;
    let path = path.trim();
    if path.is_empty() {
        return None;
    }

    match message {
        "Copied (new)" | "Copied (replaced existing)" => {
            Some(RcloneEvent::Copied(path.to_string()))
        }
        "Deleted" => Some(RcloneEvent::Deleted(path.to_string())),
        _ => None,
    }
}

#[derive(Serialize)]
struct RunRequest<'a> {
    user: &'a str,
    changed: &'a [String],
    deleted: &'a [String],
}

fn post_run(
    api_url: &str,
    user: &str,
    changed: &[String],
    deleted: &[String],
    verbose: bool,
) -> Result<()> {
    let url = format!("{}/run", api_url.trim_end_matches('/'));
    let body = RunRequest {
        user,
        changed,
        deleted,
    };
    let payload = serde_json::to_value(&body).context("serializing run request")?;
    if verbose {
        ui::verbose(&format!("POST {url}"));
        ui::verbose(&format!("body: {payload}"));
    }

    match ureq::post(&url).send_json(payload) {
        Ok(_) => Ok(()),
        Err(ureq::Error::Status(code, resp)) => bail!(
            "POST {url} returned {code}: {}",
            resp.into_string().unwrap_or_else(|_| "<no body>".into())
        ),
        Err(ureq::Error::Transport(t)) => bail!("POST {url} failed: {t}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_copied_new() {
        let line = "2026/04/05 08:48:06 INFO  : main.py: Copied (new)";
        assert_eq!(
            parse_rclone_line(line),
            Some(RcloneEvent::Copied("main.py".into()))
        );
    }

    #[test]
    fn parse_copied_replaced() {
        let line =
            "2026/04/05 08:48:06 INFO  : routes/users.py: Copied (replaced existing)";
        assert_eq!(
            parse_rclone_line(line),
            Some(RcloneEvent::Copied("routes/users.py".into()))
        );
    }

    #[test]
    fn parse_deleted() {
        let line = "2026/04/05 08:48:06 INFO  : old_handler.py: Deleted";
        assert_eq!(
            parse_rclone_line(line),
            Some(RcloneEvent::Deleted("old_handler.py".into()))
        );
    }

    #[test]
    fn parse_noise_returns_none() {
        assert_eq!(parse_rclone_line(""), None);
        assert_eq!(
            parse_rclone_line(
                "2026/04/05 08:48:06 INFO  : \n\
                 Transferred:              0 B / 0 B, -, 0 B/s, ETA -"
            ),
            None
        );
        assert_eq!(
            parse_rclone_line("2026/04/05 08:48:06 NOTICE : something happened"),
            None
        );
    }

    #[test]
    fn parse_path_with_spaces() {
        let line = "2026/04/05 08:48:06 INFO  : my notes/todo list.md: Copied (new)";
        assert_eq!(
            parse_rclone_line(line),
            Some(RcloneEvent::Copied("my notes/todo list.md".into()))
        );
    }

    #[test]
    fn parse_path_with_colon() {
        // rsplit_once(": ") preserves colons in the path segment.
        let line = "2026/04/05 08:48:06 INFO  : weird:name.py: Copied (new)";
        assert_eq!(
            parse_rclone_line(line),
            Some(RcloneEvent::Copied("weird:name.py".into()))
        );
    }

    #[test]
    fn parse_unknown_message_returns_none() {
        let line = "2026/04/05 08:48:06 INFO  : main.py: Renamed to foo.py";
        assert_eq!(parse_rclone_line(line), None);
    }
}
