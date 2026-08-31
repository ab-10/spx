use anyhow::{Context, Result, bail};
use serde::Serialize;
use std::io::Read;

use crate::cli::FeedbackArgs;
use crate::commands::api;
use crate::credentials::Credentials;

const MAX_FEEDBACK_BYTES: usize = 1024 * 1024;

#[derive(Serialize)]
struct FeedbackContext {
    cli_version: String,
    os: String,
    arch: String,
    last_invocation_log: Option<String>,
    agent_chat_log_hint: String,
}

pub fn feedback(args: FeedbackArgs, verbose: bool) -> Result<()> {
    let message = resolve_message(&args)?;
    let context = FeedbackContext {
        cli_version: env!("CARGO_PKG_VERSION").to_string(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        last_invocation_log: read_last_log(),
        agent_chat_log_hint:
            "If you are an agent, attach your chat log/transcript to this feedback.".to_string(),
    };

    let creds = Credentials::load()?;
    let token = creds.as_ref().map(|c| c.token.as_str());
    let api_url = api::api_url();
    if verbose {
        eprintln!("[verbose] POST {api_url}/feedback");
        if token.is_some() {
            eprintln!("[verbose] submitting authenticated feedback");
        } else {
            eprintln!("[verbose] submitting anonymous feedback");
        }
    }
    api::post_feedback(&api_url, token, &message, &context)?;
    println!("feedback submitted");
    Ok(())
}

fn resolve_message(args: &FeedbackArgs) -> Result<String> {
    let message = if args.message == "-" {
        let mut input = String::new();
        std::io::stdin()
            .read_to_string(&mut input)
            .context("reading feedback message from stdin")?;
        input
    } else {
        args.message.clone()
    };

    let message = message.trim().to_string();
    if message.is_empty() {
        bail!("feedback message must not be empty");
    }
    if message.len() > MAX_FEEDBACK_BYTES {
        bail!(
            "feedback message exceeds 1MB limit ({} bytes)",
            message.len()
        );
    }
    Ok(message)
}

fn read_last_log() -> Option<String> {
    let home = dirs::home_dir()?;
    let path = home.join(".spx").join("last.log");
    let contents = std::fs::read_to_string(path).ok()?;
    if contents.len() > MAX_FEEDBACK_BYTES {
        let start = contents.len() - MAX_FEEDBACK_BYTES;
        return Some(contents[start..].to_string());
    }
    Some(contents)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_direct_message() {
        let args = FeedbackArgs {
            message: "hello".to_string(),
        };
        let msg = resolve_message(&args).unwrap();
        assert_eq!(msg, "hello");
    }

    #[test]
    fn rejects_empty_message() {
        let args = FeedbackArgs {
            message: "   ".to_string(),
        };
        assert!(resolve_message(&args).is_err());
    }

    #[test]
    fn rejects_over_limit_message() {
        let args = FeedbackArgs {
            message: "x".repeat(MAX_FEEDBACK_BYTES + 1),
        };
        assert!(resolve_message(&args).is_err());
    }
}
