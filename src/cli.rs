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
    /// Create a new spx project with FastAPI scaffolding
    New(NewArgs),
    /// Authenticate with GitHub
    Login,
}

#[derive(Parser)]
pub struct RunArgs {
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Parser)]
pub struct NewArgs {
    /// Name for the new project (becomes the directory name)
    pub name: String,
}
