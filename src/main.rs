mod cli;
mod commands;
mod config;
mod docker;
mod ui;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command};

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::New(args) => commands::new::run(args),
        Command::Run(args) => commands::run::run(args),
        Command::Preview(args) => commands::preview::run(args),
        Command::Deploy(args) => commands::deploy::run(args),
    }
}
