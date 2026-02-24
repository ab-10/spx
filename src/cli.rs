use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "spawn",
    about = "CLI that fully sets up a project for agentic development",
    version,
    propagate_version = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Output results as JSON (for scripting and editor integrations)
    #[arg(long, global = true)]
    pub json: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize a new spawn project — container, Next.js, auth, and optionally full cloud wiring
    Init {
        /// Project name (defaults to current directory name)
        name: Option<String>,

        /// Local scaffold only — no database, no Vercel, no GitHub.
        /// Cloud wiring runs automatically on first `spawn deploy` or `spawn preview`.
        #[arg(long)]
        local: bool,
    },

    /// Launch a tool inside the container
    Run {
        /// Tool to run (currently: "claude")
        target: String,
    },

    /// Deploy a shareable Vercel preview URL from current working state
    Preview {
        /// Tear down the preview deployment
        #[arg(long)]
        close: bool,
    },

    /// Test-gated push to main and production deployment
    Deploy {
        /// Skip the test gate
        #[arg(long)]
        force: bool,
    },
}
