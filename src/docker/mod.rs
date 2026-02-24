pub mod container;
pub mod image;

use crate::error::Result;
use crate::output::Output;

#[cfg(test)]
use mockall::automock;

/// Options for creating a new container.
pub struct CreateContainerOpts {
    pub image: String,
    pub name: String,
    pub working_dir: String,
    pub port_bindings: Vec<(u16, u16)>, // (host, container)
    pub bind_mounts: Vec<(String, String)>, // (host_path, container_path)
    pub env: Vec<String>,
}

/// Abstraction over a container runtime (Docker via Bollard).
#[cfg_attr(test, automock)]
#[allow(async_fn_in_trait)]
pub trait ContainerRuntime {
    /// Verify the Docker daemon is reachable.
    async fn ensure_running(&self) -> Result<()>;

    /// Pull an image with streaming progress output.
    async fn pull_image(&self, image: &str, output: &Output) -> Result<()>;

    /// Create a container from the given options. Returns the container ID.
    async fn create_container(&self, opts: CreateContainerOpts) -> Result<String>;

    /// Start a container by ID.
    async fn start_container(&self, id: &str) -> Result<()>;

    /// Stop a container by ID.
    async fn stop_container(&self, id: &str) -> Result<()>;

    /// Check if a container is running.
    async fn is_container_running(&self, id: &str) -> Result<bool>;

    /// Execute a command in a container, streaming output. Returns exit code.
    async fn exec_in_container(&self, id: &str, cmd: Vec<&str>, output: &Output) -> Result<i64>;

    /// Execute an interactive command with TTY passthrough.
    async fn exec_interactive(&self, id: &str, cmd: Vec<&str>) -> Result<i64>;

    /// Copy data (as a tar archive) into a container at the given path.
    async fn copy_to_container(&self, id: &str, path: &str, data: &[u8]) -> Result<()>;
}

/// Bollard-based Docker runtime.
pub struct BollardRuntime {
    client: bollard::Docker,
}

impl BollardRuntime {
    pub fn connect() -> Result<Self> {
        let client =
            bollard::Docker::connect_with_local_defaults().map_err(|_| crate::error::SpawnError::DockerNotRunning)?;
        Ok(Self { client })
    }

    pub fn client(&self) -> &bollard::Docker {
        &self.client
    }
}
