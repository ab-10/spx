use miette::Diagnostic;
use thiserror::Error;

#[derive(Error, Diagnostic, Debug)]
pub enum SpawnError {
    #[error("Docker is not running")]
    #[diagnostic(help("Start Docker Desktop or the Docker daemon, then try again."))]
    DockerNotRunning,

    #[error("Container is not running")]
    #[diagnostic(help(
        "Run `spawn init --local` to create a container, or check `docker ps` for existing containers."
    ))]
    ContainerNotRunning,

    #[error("Failed to pull image `{image}`: {reason}")]
    #[diagnostic(help("Check your internet connection and that Docker Hub is reachable."))]
    ImagePull { image: String, reason: String },

    #[error("Project not initialized")]
    #[diagnostic(help("Run `spawn init <name> --local` to initialize a project first."))]
    NotInitialized,

    #[error("Container command failed with exit code {code}")]
    #[diagnostic(help("Check the command output above for details."))]
    ExecFailed { code: i64 },

    #[error("Failed to create container: {reason}")]
    #[diagnostic(help("Check Docker has enough resources and the image exists."))]
    ContainerCreate { reason: String },

    #[error("Failed to start container: {reason}")]
    #[diagnostic(help("Run `docker logs <container>` for details."))]
    ContainerStart { reason: String },

    #[error(transparent)]
    #[diagnostic(code(spawn::io))]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    #[diagnostic(code(spawn::docker))]
    Docker(#[from] bollard::errors::Error),

    #[error(transparent)]
    #[diagnostic(code(spawn::json))]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, SpawnError>;
