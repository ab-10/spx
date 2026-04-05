use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const CONFIG_FILE: &str = "spx.config.json";
const STATE_DIR: &str = ".spx";
const STATE_FILE: &str = "state.json";

// --- Shared, version-controlled config (spx.config.json) ---

#[derive(Debug, Serialize, Deserialize)]
pub struct SpxConfig {
    pub project_name: String,
}

impl SpxConfig {
    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join(CONFIG_FILE);
        let contents =
            std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let config: Self =
            serde_json::from_str(&contents).with_context(|| format!("parsing {}", path.display()))?;
        Ok(config)
    }

    pub fn save(&self, dir: &Path) -> Result<()> {
        let path = dir.join(CONFIG_FILE);
        let contents = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, contents)
            .with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    pub fn path(dir: &Path) -> PathBuf {
        dir.join(CONFIG_FILE)
    }

    pub fn exists(dir: &Path) -> bool {
        dir.join(CONFIG_FILE).exists()
    }
}

// --- Local, gitignored state (.spx/state.json) ---

#[derive(Debug, Serialize, Deserialize)]
pub struct LocalState {
    pub container_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_ip: Option<String>,
}

impl LocalState {
    /// Get the dev server URL label (e.g. "192.168.1.2:3000").
    pub fn dev_url(&self) -> Option<String> {
        self.container_ip.as_ref().map(|ip| format!("{ip}:3000"))
    }
}

impl LocalState {
    pub fn load(dir: &Path) -> Result<Self> {
        let path = Self::path(dir);
        let contents =
            std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let state: Self =
            serde_json::from_str(&contents).with_context(|| format!("parsing {}", path.display()))?;
        Ok(state)
    }

    pub fn save(&self, dir: &Path) -> Result<()> {
        let state_dir = dir.join(STATE_DIR);
        std::fs::create_dir_all(&state_dir)
            .with_context(|| format!("creating {}", state_dir.display()))?;
        let path = Self::path(dir);
        let contents = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, contents)
            .with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    pub fn exists(dir: &Path) -> bool {
        Self::path(dir).exists()
    }

    fn path(dir: &Path) -> PathBuf {
        dir.join(STATE_DIR).join(STATE_FILE)
    }

    /// Create a fresh LocalState with a unique petname-based container name.
    pub fn init(project_name: &str) -> Self {
        let suffix = petname::petname(3, "-").unwrap_or_else(|| "container".to_string());
        LocalState {
            container_name: format!("spx-{project_name}-{suffix}"),
            container_id: None,
            container_ip: None,
        }
    }
}

// --- Migration from old combined format ---

/// If spx.config.json contains `container_id` (old combined format),
/// split it into the new two-file layout. Preserves the existing container_name
/// so running containers aren't orphaned. Idempotent.
pub fn migrate_if_needed(dir: &Path) -> Result<()> {
    if LocalState::exists(dir) {
        return Ok(());
    }

    let config_path = dir.join(CONFIG_FILE);
    if !config_path.exists() {
        return Ok(());
    }

    let contents = std::fs::read_to_string(&config_path)
        .with_context(|| format!("reading {}", config_path.display()))?;
    let raw: serde_json::Value = serde_json::from_str(&contents)
        .with_context(|| format!("parsing {}", config_path.display()))?;

    // Only migrate if old format (has container_id key)
    if raw.get("container_id").is_none() {
        return Ok(());
    }

    let project_name = raw["project_name"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();

    let container_name = raw
        .get("container_name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("spx-{project_name}"));

    let container_id = raw.get("container_id").and_then(|v| v.as_str()).map(|s| s.to_string());

    let state = LocalState {
        container_name,
        container_id,
        container_ip: None,
    };
    state.save(dir)?;

    // Rewrite spx.config.json with only project_name
    let config = SpxConfig { project_name };
    config.save(dir)?;

    ensure_gitignore_has_spx(dir)?;

    Ok(())
}

/// Append `.spx/` to .gitignore if not already present.
pub fn ensure_gitignore_has_spx(dir: &Path) -> Result<()> {
    let gitignore_path = dir.join(".gitignore");
    if gitignore_path.exists() {
        let contents = std::fs::read_to_string(&gitignore_path)?;
        if contents.lines().any(|line| line.trim() == ".spx/" || line.trim() == ".spx") {
            return Ok(());
        }
        let mut new_contents = contents;
        if !new_contents.ends_with('\n') {
            new_contents.push('\n');
        }
        new_contents.push_str(".spx/\n");
        std::fs::write(&gitignore_path, new_contents)?;
    } else {
        std::fs::write(&gitignore_path, ".spx/\n")?;
    }
    Ok(())
}

/// When no spx.config.json exists but the directory looks like a project,
/// derive project_name from the directory name and create the config.
pub fn recover_config(dir: &Path) -> Result<SpxConfig> {
    let has_package_json = dir.join("package.json").exists();
    let has_git = dir.join(".git").exists();

    if !has_package_json && !has_git {
        anyhow::bail!(
            "No spx.config.json found and directory doesn't look like a project. Run `spx new` first."
        );
    }

    let project_name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let config = SpxConfig { project_name };
    config.save(dir)?;
    ensure_gitignore_has_spx(dir)?;

    Ok(config)
}
