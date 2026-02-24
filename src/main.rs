mod cli;
mod cloud;
mod commands;
mod config;
mod docker;
mod error;
mod output;

use clap::Parser;
use cli::{Cli, Commands};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Init { name, local } => commands::init::run(name, local, cli.json).await,
        Commands::Run { target } => commands::run::run(target, cli.json).await,
        Commands::Preview { close } => commands::preview::run(close, cli.json).await,
        Commands::Deploy { force } => commands::deploy::run(force, cli.json).await,
    };

    if let Err(e) = result {
        if cli.json {
            let err_json = serde_json::json!({
                "error": true,
                "message": format!("{e:#}"),
            });
            eprintln!("{}", serde_json::to_string_pretty(&err_json).unwrap());
        } else {
            output::error(&format!("{e:#}"));
        }
        std::process::exit(1);
    }
}
