use clap::{Parser, Subcommand};


#[derive(Parser)]
#[command(name = "spx", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Output as JSON for scripting and editor integrations
    #[arg(long, global = true)]
    pub json: bool,

    /// Print verbose debug output (useful when a command hangs)
    #[arg(short, long, global = true)]
    pub verbose: bool,
}

#[derive(Subcommand)]
pub enum Command {
    /// Sync the project diff to GCS and request a preview-env run
    Run(RunArgs),
}

#[derive(Parser)]
pub struct RunArgs {
    /// Set the user identity for this project (persisted to .spx/state.json)
    #[arg(long)]
    pub user: Option<String>,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}
