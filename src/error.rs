use thiserror::Error;

#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum SpawnError {
    #[error("Docker is not installed or not running. Install Docker: https://docs.docker.com/get-docker/")]
    DockerNotFound,

    #[error("Docker daemon is not running. Start Docker and try again.")]
    DockerNotRunning,

    #[error("Container '{0}' not found")]
    ContainerNotFound(String),

    #[error("Container '{0}' is not running")]
    ContainerNotRunning(String),

    #[error("Project '{0}' already exists in current directory")]
    ProjectAlreadyExists(String),

    #[error("Not inside a spawn project. Run `spawn init` first.")]
    NotASpawnProject,

    #[error("Cloud wiring not configured. Run `spawn init` without --local or use `spawn deploy` to connect.")]
    CloudNotConfigured,

    #[error("Vercel CLI not found. Install it: npm i -g vercel")]
    VercelCliNotFound,

    #[error("GitHub CLI not found. Install it: https://cli.github.com/")]
    GhCliNotFound,

    #[error("Vercel authentication required. Run `vercel login` first.")]
    VercelNotAuthenticated,

    #[error("GitHub authentication required. Run `gh auth login` first.")]
    GhNotAuthenticated,

    #[error("Tests failed. Fix failing tests before deploying, or use --force to skip.")]
    TestsFailed,

    #[error("Command failed: {command}\n{stderr}")]
    CommandFailed { command: String, stderr: String },

    #[error("{0}")]
    Other(String),
}
