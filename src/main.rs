mod cli;
mod commands;
mod config;
mod runtime;
mod ui;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command};

fn main() -> Result<()> {
    let cli = Cli::parse();
    let verbose = cli.verbose;

    match cli.command {
        Command::New(args) => commands::new::run(args, verbose),
        Command::Link(args) => commands::link::run(args, verbose),
        Command::Claude(args) => commands::claude::run(args, verbose),
        Command::Shell(args) => commands::shell::run(args, verbose),
    }
}
