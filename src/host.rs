use anyhow::{bail, Context, Result};
use std::process::{Command, Stdio};

/// Verify that a CLI tool is on PATH; bail with install instructions if not.
pub fn require_tool(name: &str, install_hint: &str) -> Result<()> {
    if which::which(name).is_err() {
        bail!(
            "`{name}` is not installed or not in PATH.\n\
             Install it: {install_hint}"
        );
    }
    Ok(())
}

/// Run an auth-check command and return whether it succeeded.
pub fn check_auth(cmd: &str, args: &[&str]) -> bool {
    Command::new(cmd)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Run a command with inherited stdio (for interactive login flows).
pub fn run_interactive(cmd: &str, args: &[&str]) -> Result<()> {
    let display = format!("{cmd} {}", args.join(" "));
    crate::ui::stream_header(&display);

    let status = Command::new(cmd)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("failed to run '{display}'"))?;

    if !status.success() {
        bail!("Command '{display}' failed with exit code {:?}", status.code());
    }
    Ok(())
}

/// Run a command streaming output to stderr, bail on failure.
pub fn run_streaming(cmd: &str, args: &[&str]) -> Result<()> {
    let display = format!("{cmd} {}", args.join(" "));
    crate::ui::stream_header(&display);

    let status = Command::new(cmd)
        .args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("failed to run '{display}'"))?;

    if !status.success() {
        bail!("Command '{display}' failed with exit code {:?}", status.code());
    }
    Ok(())
}

/// Run a command and capture its stdout as a trimmed string.
pub fn run_capture(cmd: &str, args: &[&str]) -> Result<String> {
    let display = format!("{cmd} {}", args.join(" "));

    let output = Command::new(cmd)
        .args(args)
        .stderr(Stdio::inherit())
        .output()
        .with_context(|| format!("failed to run '{display}'"))?;

    if !output.status.success() {
        bail!("Command '{display}' failed with exit code {:?}", output.status.code());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Run a command with data piped to stdin.
pub fn run_with_stdin(cmd: &str, args: &[&str], stdin_data: &str) -> Result<()> {
    use std::io::Write;

    let display = format!("{cmd} {}", args.join(" "));
    crate::ui::stream_header(&display);

    let mut child = Command::new(cmd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("failed to spawn '{display}'"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(stdin_data.as_bytes())
            .with_context(|| format!("failed to write stdin to '{display}'"))?;
    }

    let status = child.wait().with_context(|| format!("failed to wait for '{display}'"))?;

    if !status.success() {
        bail!("Command '{display}' failed with exit code {:?}", status.code());
    }
    Ok(())
}
