use crate::error::{Result, SpawnError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const CONFIG_FILE: &str = "spawn.config.json";

#[derive(Debug, Serialize, Deserialize)]
pub struct SpawnConfig {
    pub project_name: String,
    pub container: ContainerConfig,
    pub local_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloud: Option<CloudConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ContainerConfig {
    pub image: String,
    pub container_id: String,
    pub container_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CloudConfig {
    pub vercel_project_id: Option<String>,
    pub github_repo: Option<String>,
}

impl SpawnConfig {
    /// Load config from spawn.config.json in the given directory (or current dir).
    pub fn load(dir: Option<&Path>) -> Result<Self> {
        let path = config_path(dir);
        if !path.exists() {
            return Err(SpawnError::NotInitialized);
        }
        let content = std::fs::read_to_string(&path)?;
        let config: SpawnConfig = serde_json::from_str(&content)?;
        Ok(config)
    }

    /// Save config to spawn.config.json in the given directory (or current dir).
    pub fn save(&self, dir: Option<&Path>) -> Result<()> {
        let path = config_path(dir);
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    /// Returns true if cloud wiring hasn't been configured yet.
    pub fn needs_cloud_wiring(&self) -> bool {
        self.cloud.is_none()
    }
}

fn config_path(dir: Option<&Path>) -> PathBuf {
    match dir {
        Some(d) => d.join(CONFIG_FILE),
        None => PathBuf::from(CONFIG_FILE),
    }
}
