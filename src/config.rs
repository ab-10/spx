// Legacy local state and migration helpers. Some functions are unused after
// the auth cutover but kept for potential future migration paths.
#![allow(dead_code)]
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const OLD_CONFIG_FILE: &str = "spx.config.json";
const STATE_DIR: &str = ".spx";
const STATE_FILE: &str = "state.json";

// --- Local, gitignored state (.spx/state.json) ---

#[derive(Debug, Serialize, Deserialize)]
pub struct LocalState {
    pub project_name: String,
    pub container_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_ip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
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
            project_name: project_name.to_string(),
            container_name: format!("spx-{project_name}-{suffix}"),
            container_id: None,
            container_ip: None,
            user: None,
        }
    }
}

// --- Migration from old formats ---

/// Migrate old config layouts into the current single-file `.spx/state.json`.
///
/// Handles two legacy formats:
/// 1. Combined format: `spx.config.json` with `container_id` (very old)
/// 2. Two-file format: `spx.config.json` + `.spx/state.json` (previous)
///
/// After migration, `spx.config.json` is deleted. Idempotent.
pub fn migrate_if_needed(dir: &Path) -> Result<()> {
    let old_config_path = dir.join(OLD_CONFIG_FILE);
    if !old_config_path.exists() {
        return Ok(());
    }

    let contents = std::fs::read_to_string(&old_config_path)
        .with_context(|| format!("reading {}", old_config_path.display()))?;
    let raw: serde_json::Value = serde_json::from_str(&contents)
        .with_context(|| format!("parsing {}", old_config_path.display()))?;

    let project_name = raw["project_name"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();

    if LocalState::exists(dir) {
        // Two-file format: state.json exists but may lack project_name.
        let mut state = LocalState::load(dir)?;
        if !state_has_project_name(dir)? {
            state.project_name = project_name;
            state.save(dir)?;
        }
    } else if raw.get("container_id").is_some() {
        // Very old combined format: everything in spx.config.json.
        let container_name = raw
            .get("container_name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("spx-{project_name}"));
        let container_id = raw.get("container_id").and_then(|v| v.as_str()).map(|s| s.to_string());

        let state = LocalState {
            project_name,
            container_name,
            container_id,
            container_ip: None,
            user: None,
        };
        state.save(dir)?;
        ensure_gitignore_has_spx(dir)?;
    } else {
        // spx.config.json exists but no state.json and no container_id —
        // just a bare config. Create state from it.
        let state = LocalState::init(&project_name);
        state.save(dir)?;
        ensure_gitignore_has_spx(dir)?;
    }

    // Remove the old config file.
    std::fs::remove_file(&old_config_path)
        .with_context(|| format!("removing {}", old_config_path.display()))?;

    Ok(())
}

/// Check whether the existing state.json already has a `project_name` field.
fn state_has_project_name(dir: &Path) -> Result<bool> {
    let path = dir.join(STATE_DIR).join(STATE_FILE);
    let contents = std::fs::read_to_string(&path)?;
    let raw: serde_json::Value = serde_json::from_str(&contents)?;
    Ok(raw.get("project_name").is_some())
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

/// When no `.spx/state.json` exists but the directory looks like a project,
/// derive project_name from the directory name and create state.
pub fn recover_state(dir: &Path) -> Result<LocalState> {
    let has_package_json = dir.join("package.json").exists();
    let has_git = dir.join(".git").exists();

    if !has_package_json && !has_git {
        anyhow::bail!(
            "No .spx/state.json found and directory doesn't look like a project. Run `spx new` first."
        );
    }

    let project_name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let state = LocalState::init(&project_name);
    state.save(dir)?;
    ensure_gitignore_has_spx(dir)?;

    Ok(state)
}
