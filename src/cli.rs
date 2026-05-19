use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "spx",
    version,
    about,
    long_about = "Deploy FastAPI backends to the SPX production runtime. Deployments are remote services with stable project URLs; closing the local CLI does not intentionally stop the remote service."
)]
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
    /// Deploy the project to its stable SPX URL
    Run(RunArgs),
    /// Create a new SPX project with FastAPI scaffolding and deploy it
    New(NewArgs),
    /// Authenticate via GitHub OAuth, or with a registration code
    Login(LoginArgs),
    /// Stop a running remote service by deployment slug
    Kill(KillArgs),
    /// List your running remote services
    Ps,
}

#[derive(Parser)]
#[command(
    about = "Deploy the project to its stable SPX URL",
    long_about = "Packages the current directory and deploys the selected Python entry file to the SPX production runtime. Re-running this command for the same project replaces the running remote service at the same project URL."
)]
pub struct RunArgs {
    /// Path to the Python entry file (relative to CWD)
    pub filename: PathBuf,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Parser)]
#[command(
    about = "Create a new SPX project and deploy it",
    long_about = "Creates a FastAPI project, installs dependencies, and performs the first deployment to a stable SPX project URL."
)]
pub struct NewArgs {
    /// Name for the new project (becomes the directory name)
    pub name: String,
}

#[derive(Parser)]
#[command(
    about = "Stop a running remote service",
    long_about = "Stops the running remote service for a deployment slug and removes its active routing while stopped. Local project files and saved project identity are not deleted."
)]
pub struct KillArgs {
    /// Deployment slug to stop
    pub deployment_slug: String,
}

#[derive(Parser)]
pub struct LoginArgs {
    /// Redeem a registration code to bypass GitHub OAuth
    #[arg(long)]
    pub code: Option<String>,
}
