mod cli;
mod commands;
mod config;
mod credentials;
mod ui;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command};

fn main() -> Result<()> {
    let cli = Cli::parse();
    let verbose = cli.verbose;

    match cli.command {
        Command::Run(args) => commands::run::run(args, verbose),
        Command::New(args) => commands::new::new_project(args, verbose),
        Command::Login => commands::login::login(verbose),
    }
}
