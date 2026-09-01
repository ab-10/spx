use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[derive(Debug, Serialize, Deserialize)]
pub struct Credentials {
    pub username: String,
    pub token: String,
}

impl Credentials {
    fn path() -> Result<PathBuf> {
        let home = dirs::home_dir().context("could not determine home directory")?;
        Ok(home.join(".spx").join("credentials.json"))
    }

    pub fn load() -> Result<Option<Self>> {
        let path = Self::path()?;
        if !path.exists() {
            return Ok(None);
        }
        Self::harden_permissions(&path)?;
        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let creds: Self = serde_json::from_str(&contents)
            .with_context(|| format!("parsing {}", path.display()))?;
        Ok(Some(creds))
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
            Self::set_dir_private(parent)?;
        }
        let contents = serde_json::to_string_pretty(self)?;
        let tmp_path = path.with_extension("json.tmp");
        std::fs::write(&tmp_path, contents)
            .with_context(|| format!("writing {}", tmp_path.display()))?;
        Self::set_file_private(&tmp_path)?;
        std::fs::rename(&tmp_path, &path)
            .with_context(|| format!("renaming {} to {}", tmp_path.display(), path.display()))?;
        Self::set_file_private(&path)?;
        Ok(())
    }

    pub fn require() -> Result<Self> {
        match Self::load()? {
            Some(creds) => Ok(creds),
            None => bail!(
                "not logged in (couldn't find token at `~/.spx/credentials.json`)\n\
                 Run `spx login` to authenticate with GitHub."
            ),
        }
    }

    fn harden_permissions(path: &std::path::Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            Self::set_dir_private(parent)?;
        }
        Self::set_file_private(path)
    }

    #[cfg(unix)]
    fn set_dir_private(path: &std::path::Path) -> Result<()> {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .with_context(|| format!("setting permissions on {}", path.display()))
    }

    #[cfg(not(unix))]
    fn set_dir_private(_path: &std::path::Path) -> Result<()> {
        Ok(())
    }

    #[cfg(unix)]
    fn set_file_private(path: &std::path::Path) -> Result<()> {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("setting permissions on {}", path.display()))
    }

    #[cfg(not(unix))]
    fn set_file_private(_path: &std::path::Path) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn save_writes_private_credentials() {
        let tmp_dir = tempfile::tempdir().expect("tempdir");
        let previous_home = std::env::var_os("HOME");
        unsafe { std::env::set_var("HOME", tmp_dir.path()) };

        Credentials {
            username: "alice".to_string(),
            token: "secret".to_string(),
        }
        .save()
        .unwrap();

        let dir_meta = std::fs::metadata(tmp_dir.path().join(".spx")).unwrap();
        let file_meta = std::fs::metadata(tmp_dir.path().join(".spx/credentials.json")).unwrap();
        assert_eq!(dir_meta.permissions().mode() & 0o777, 0o700);
        assert_eq!(file_meta.permissions().mode() & 0o777, 0o600);

        match previous_home {
            Some(home) => unsafe { std::env::set_var("HOME", home) },
            None => unsafe { std::env::remove_var("HOME") },
        }
    }
}
