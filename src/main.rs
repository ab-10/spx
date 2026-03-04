mod cli;
mod commands;
mod config;
mod docker;
mod host;
mod ui;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command};

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::New(args) => commands::new::run(args),
        Command::Link(args) => commands::link::run(args),
        Command::Claude(args) => commands::claude::run(args),
        Command::Shell(args) => commands::shell::run(args),
    }
}
