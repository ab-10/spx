use clap::{Parser, Subcommand};
use std::path::PathBuf;


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
    /// Package and deploy the project to a preview environment
    Run(RunArgs),
    /// Create a new spx project with FastAPI scaffolding
    New(NewArgs),
    /// Authenticate via GitHub OAuth, or with a registration code
    Login(LoginArgs),
    /// Kill a running deproc by its pet name
    Kill(KillArgs),
    /// List your running deployments
    Ps,
}

#[derive(Parser)]
pub struct RunArgs {
    /// Path to the Python entry file (relative to CWD)
    pub filename: PathBuf,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Parser)]
pub struct NewArgs {
    /// Name for the new project (becomes the directory name)
    pub name: String,
}

#[derive(Parser)]
pub struct KillArgs {
    /// Pet name of the deproc/VM to kill
    pub pet_name: String,
}

#[derive(Parser)]
pub struct LoginArgs {
    /// Redeem a registration code to bypass GitHub OAuth
    #[arg(long)]
    pub code: Option<String>,
}
