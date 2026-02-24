use clap::{Parser, Subcommand};

/// CLI that fully sets up a project for agentic development.
#[derive(Parser)]
#[command(name = "spawn", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Output as JSON for scripting and editor integrations
    #[arg(long, global = true)]
    pub json: bool,
}

#[derive(Subcommand)]
pub enum Command {
    /// Full setup — container, DB, auth, GitHub, Vercel
    Init(InitArgs),
    /// Run a tool inside the container
    Run(RunArgs),
    /// Shareable Vercel preview URL from current working state
    Preview(PreviewArgs),
    /// Test-gated push to main → production
    Deploy(DeployArgs),
}

#[derive(Parser)]
pub struct InitArgs {
    /// Project name (defaults to current directory name)
    pub name: Option<String>,

    /// Local scaffold only — cloud wiring deferred to first deploy/preview
    #[arg(long)]
    pub local: bool,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Parser)]
pub struct RunArgs {
    /// Tool to run (e.g. "claude")
    pub tool: String,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Parser)]
pub struct PreviewArgs {
    /// Tear down the preview deployment
    #[arg(long)]
    pub close: bool,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Parser)]
pub struct DeployArgs {
    /// Skip the test gate
    #[arg(long)]
    pub force: bool,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}
