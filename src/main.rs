mod cli;
mod commands;
mod config;
mod docker;
mod error;
mod output;
mod templates;

use clap::Parser;
use cli::{Cli, Commands};
use output::Output;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let output = Output::new(cli.json);

    let result = match cli.command {
        Commands::Init { name, local } => commands::init::run(name, local, &output).await,
        Commands::Run { tool } => commands::run::run(tool, &output).await,
        Commands::Preview => {
            output.warn("spawn preview is not yet implemented.");
            output.next_step("This command will create a shareable Vercel preview deployment.");
            Ok(())
        }
        Commands::Deploy => {
            output.warn("spawn deploy is not yet implemented.");
            output.next_step("This command will run tests and deploy to production via Vercel.");
            Ok(())
        }
    };

    if let Err(e) = result {
        if output.is_json() {
            output.error(&format!("{e}"));
        } else {
            // Use miette for pretty error display
            let report: miette::Report = e.into();
            eprintln!("{report:?}");
        }
        std::process::exit(1);
    }
}
