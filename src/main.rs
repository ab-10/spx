mod cli;
mod commands;
mod credentials;
mod ui;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command, PubCreateArgs};

fn main() -> Result<()> {
    let cli = Cli::parse();
    let verbose = cli.verbose;

    match (cli.command, cli.path) {
        (Some(Command::Create(args)), None) => commands::pub_cmd::create(args, verbose),
        (Some(Command::Update(args)), None) => commands::pub_cmd::update(args, verbose),
        (Some(Command::Delete(args)), None) => commands::pub_cmd::delete(args, verbose),
        (Some(Command::List), None) => commands::pub_cmd::list(verbose),
        (Some(Command::Login(args)), None) => match args.code {
            Some(code) => commands::login::login_with_code(&code, verbose),
            None => commands::login::login(verbose),
        },
        (Some(Command::Feedback(args)), None) => commands::feedback::feedback(args, verbose),
        (Some(Command::Subscribe), None) => commands::subscribe::subscribe(verbose),
        (None, Some(path)) => commands::pub_cmd::create(PubCreateArgs { path }, verbose),
        (Some(_), Some(_)) => anyhow::bail!("pass either `spx PATH` or a subcommand, not both"),
        (None, None) => anyhow::bail!("missing path. Use `spx PATH` or `spx list`."),
    }
}
