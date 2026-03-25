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

    /// Print verbose debug output (useful when a command hangs)
    #[arg(short, long, global = true)]
    pub verbose: bool,
}

#[derive(Subcommand)]
pub enum Command {
    /// Create a new project — container + scaffold
    New(NewArgs),
    /// Wire project to GitHub + Vercel for continuous deployment
    Link(LinkArgs),
    /// Launch a Claude Code session inside the container
    Claude(ClaudeArgs),
    /// Open an interactive shell inside the container
    Shell(ShellArgs),
}

#[derive(Parser)]
pub struct NewArgs {
    /// Project name
    pub name: String,

    /// Skip interactive prompts and the shell drop-in at the end (for CI/scripting)
    #[arg(long)]
    pub non_interactive: bool,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Parser)]
pub struct LinkArgs {
    /// Skip interactive prompts (for CI/scripting)
    #[arg(long)]
    pub non_interactive: bool,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Parser)]
pub struct ClaudeArgs {
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Parser)]
pub struct ShellArgs {
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}
