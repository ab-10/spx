use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const CONFIG_FILE: &str = "spawn.config.json";

fn default_host_port() -> u16 {
    3000
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SpawnConfig {
    /// Project name
    pub name: String,

    /// Whether cloud wiring has been completed
    #[serde(default)]
    pub cloud_connected: bool,

    /// Docker container name for this project
    #[serde(default)]
    pub container_name: String,

    /// Docker image used
    #[serde(default)]
    pub docker_image: String,

    /// Host port mapped to container port 3000
    #[serde(default = "default_host_port")]
    pub host_port: u16,

    /// Vercel project details
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vercel: Option<VercelConfig>,

    /// GitHub repository details
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github: Option<GithubConfig>,

    /// Stack Auth details
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack_auth: Option<StackAuthConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VercelConfig {
    pub project_id: String,
    pub project_name: String,
    pub org_id: Option<String>,
    pub production_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubConfig {
    pub repo: String,
    pub owner: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackAuthConfig {
    pub project_id: String,
}

impl SpawnConfig {
    /// Load config from the given project directory.
    pub fn load(project_dir: &Path) -> Result<Self> {
        let path = project_dir.join(CONFIG_FILE);
        let contents =
            std::fs::read_to_string(&path).with_context(|| format!("Reading {}", path.display()))?;
        serde_json::from_str(&contents).with_context(|| format!("Parsing {}", path.display()))
    }

    /// Save config to the given project directory.
    pub fn save(&self, project_dir: &Path) -> Result<()> {
        let path = project_dir.join(CONFIG_FILE);
        let contents = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, contents)
            .with_context(|| format!("Writing {}", path.display()))?;
        Ok(())
    }

    /// Find the spawn.config.json in the current directory or parents.
    pub fn find() -> Result<(Self, PathBuf)> {
        let cwd = std::env::current_dir()?;
        let mut dir = cwd.as_path();
        loop {
            let candidate = dir.join(CONFIG_FILE);
            if candidate.exists() {
                let config = Self::load(dir)?;
                return Ok((config, dir.to_path_buf()));
            }
            match dir.parent() {
                Some(parent) => dir = parent,
                None => {
                    anyhow::bail!(
                        "Not inside a spawn project. Run `spawn init` first."
                    );
                }
            }
        }
    }

    pub fn is_cloud_connected(&self) -> bool {
        self.cloud_connected
    }
}
