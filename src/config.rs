use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const CONFIG_FILE: &str = "spawn.config.json";

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct SpawnConfig {
    pub project_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github_repo: Option<String>,
}

impl SpawnConfig {
    /// Load config from spawn.config.json in the given directory.
    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join(CONFIG_FILE);
        let contents =
            std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let config: Self =
            serde_json::from_str(&contents).with_context(|| format!("parsing {}", path.display()))?;
        Ok(config)
    }

    /// Save config to spawn.config.json in the given directory.
    pub fn save(&self, dir: &Path) -> Result<()> {
        let path = dir.join(CONFIG_FILE);
        let contents = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, contents)
            .with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    /// Return the path to the config file in the given directory.
    pub fn path(dir: &Path) -> PathBuf {
        dir.join(CONFIG_FILE)
    }

    /// Check if a config file exists in the given directory.
    pub fn exists(dir: &Path) -> bool {
        dir.join(CONFIG_FILE).exists()
    }
}
