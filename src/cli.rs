use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "spawn", version, about = "Set up a project for agentic development with Claude Code")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Output as newline-delimited JSON instead of styled text
    #[arg(long, global = true)]
    pub json: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize a new spawn project
    Init {
        /// Project name (defaults to current directory name)
        name: Option<String>,

        /// Local-only mode: skip cloud wiring (Vercel, GitHub, Stack Auth)
        #[arg(long)]
        local: bool,
    },

    /// Run a tool inside the spawn container
    Run {
        /// Which tool to run
        #[arg(value_enum)]
        tool: RunTool,
    },

    /// Create a shareable preview deployment
    Preview,

    /// Deploy to production (test-gated)
    Deploy,
}

#[derive(Clone, ValueEnum)]
pub enum RunTool {
    /// Interactive Claude Code session
    Claude,
}
