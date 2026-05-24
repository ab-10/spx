use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "spx",
    version,
    about,
    long_about = "Deploy FastAPI backends to the SPX production runtime. Deployments are remote services with stable project URLs; closing the local CLI does not intentionally stop the remote service. When authoring an SPX Python entrypoint, assume it is run directly via `python <file>` and must itself start and keep an HTTP server bound to `0.0.0.0` on a port in `8000-9000` (prefer `8080`)."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

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
    /// Print recent runtime logs for the current project
    Logs(LogsArgs),
    /// Manage project environment variables and secrets
    Env(EnvArgs),
    /// Manage deployment dependencies
    Uv(UvArgs),
    /// Publish standalone HTML files at unlisted URLs
    Pub(PubArgs),
}

#[derive(Parser)]
#[command(
    about = "Deploy the project to its stable SPX URL",
    long_about = "Packages the current directory and deploys the selected Python entry file to the SPX production runtime. Re-running this command for the same project replaces the running remote service at the same project URL. When authoring an SPX Python entrypoint, assume it is run directly via `python <file>` and must itself start and keep an HTTP server bound to `0.0.0.0` on a port in `8000-9000` (prefer `8080`). SPX scans 8000-9000 for user-exposed ports and binds the deployment URL to the lowest discovered listening port in range."
)]
pub struct RunArgs {
    /// Path to the Python entry file (relative to CWD)
    pub filename: PathBuf,
}

#[derive(Parser)]
#[command(
    about = "Manage project environment variables",
    long_about = "Sets, unsets, lists, and bulk-loads persisted project environment variables for SPX deployments. Commands are non-interactive by default for agent and CI safety."
)]
pub struct EnvArgs {
    #[command(subcommand)]
    pub command: EnvCommand,
}

#[derive(Subcommand)]
pub enum EnvCommand {
    /// Set a persisted env variable for this project
    Set(EnvSetArgs),
    /// Remove a persisted env variable for this project
    Unset(EnvUnsetArgs),
    /// List persisted env variable keys for this project
    List,
    /// Load env variables from a file (for example .env)
    Load(EnvLoadArgs),
}

#[derive(Parser)]
pub struct EnvSetArgs {
    /// KEY or KEY=value
    pub key_or_pair: String,

    /// Read the value from stdin
    #[arg(long)]
    pub from_stdin: bool,

    /// Read the value from local process environment variable named KEY
    #[arg(long)]
    pub from_env: bool,
}

#[derive(Parser)]
pub struct EnvUnsetArgs {
    /// Env var key to remove
    pub key: String,
}

#[derive(Parser)]
pub struct EnvLoadArgs {
    /// Path to env file (e.g. .env)
    pub file: PathBuf,
}

#[derive(Parser)]
#[command(
    about = "Manage deployment dependencies",
    long_about = "Adds, removes, and lists deployment dependencies for this project. Changes apply to the next `spx run`."
)]
pub struct UvArgs {
    #[command(subcommand)]
    pub command: UvCommand,
}

#[derive(Subcommand)]
pub enum UvCommand {
    /// Add or update a dependency requirement
    Add(UvAddArgs),
    /// Remove a dependency by package name
    Remove(UvRemoveArgs),
    /// List deployment dependencies
    List,
}

#[derive(Parser)]
pub struct UvAddArgs {
    /// Requirement specifier (e.g. httpx or "uvicorn[standard]>=0.34")
    pub requirement: String,
}

#[derive(Parser)]
pub struct UvRemoveArgs {
    /// Package name to remove
    pub name: String,
}

#[derive(Parser)]
#[command(
    about = "Publish standalone HTML files",
    long_about = "Uploads a single standalone HTML file and serves it at an unlisted pub.runspx.com URL."
)]
pub struct PubArgs {
    #[command(subcommand)]
    pub command: Option<PubCommand>,

    /// HTML file to publish. Equivalent to `spx pub create PATH`.
    pub path: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum PubCommand {
    /// Upload a new standalone HTML pub
    Create(PubCreateArgs),
    /// Replace an existing pub and keep the same URL
    Update(PubUpdateArgs),
    /// Delete an existing pub
    Delete(PubDeleteArgs),
    /// List your pubs
    List,
}

#[derive(Parser)]
pub struct PubCreateArgs {
    /// Standalone HTML file to publish
    pub path: PathBuf,
}

#[derive(Parser)]
pub struct PubUpdateArgs {
    /// Pub slug or https://{slug}.pub.runspx.com/ URL
    pub slug_or_url: String,

    /// Replacement standalone HTML file
    pub path: PathBuf,
}

#[derive(Parser)]
pub struct PubDeleteArgs {
    /// Pub slug or https://{slug}.pub.runspx.com/ URL
    pub slug_or_url: String,
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
#[command(
    about = "Print recent runtime logs for the current project",
    long_about = "Prints JSON runtime logs for the current project's latest deployment run. Defaults to the last five minutes."
)]
pub struct LogsArgs {
    /// Start of the query window as an ISO 8601 timestamp
    #[arg(long = "from")]
    pub from: Option<String>,

    /// End of the query window as an ISO 8601 timestamp
    #[arg(long)]
    pub to: Option<String>,

    /// Maximum number of log entries to return
    #[arg(long, default_value_t = 500)]
    pub limit: u32,

    /// Filter by derived severity: info or error
    #[arg(long)]
    pub severity: Option<String>,
}

#[derive(Parser)]
pub struct LoginArgs {
    /// Redeem a registration code to bypass GitHub OAuth
    #[arg(long)]
    pub code: Option<String>,
}
