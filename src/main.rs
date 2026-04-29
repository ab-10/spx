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
    let json = cli.json;

    match cli.command {
        Command::Run(args) => commands::run::run(args, verbose),
        Command::New(args) => commands::new::new_project(args, verbose),
        Command::Login(args) => match args.code {
            Some(code) => commands::login::login_with_code(&code, verbose),
            None => commands::login::login(verbose),
        },
        Command::Kill(args) => commands::kill::kill(args, verbose),
        Command::Ps => commands::ps::ps(json, verbose),
    }
}
