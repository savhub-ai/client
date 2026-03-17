use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GlobalConfig {
    pub registry: Option<String>,
    pub token: Option<String>,
}

pub fn get_config_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("SAVHUB_CONFIG_PATH") {
        return Ok(PathBuf::from(path));
    }

    let project_dirs = ProjectDirs::from("", "", "savhub")
        .context("could not resolve the savhub config directory")?;
    Ok(project_dirs.config_dir().join("config.toml"))
}

pub fn read_global_config() -> Result<Option<GlobalConfig>> {
    let candidates = vec![get_config_path()?];
    for path in candidates {
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(config) = toml::from_str::<GlobalConfig>(&raw) else {
            continue;
        };
        return Ok(Some(config));
    }
    Ok(None)
}

pub fn write_global_config(config: &GlobalConfig) -> Result<()> {
    let path = get_config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create config directory {}", parent.display()))?;
    }
    let payload = toml::to_string_pretty(config)
        .with_context(|| "failed to serialize config as TOML")?;
    fs::write(&path, format!("{payload}\n"))
        .with_context(|| format!("failed to write {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        if let Ok(metadata) = fs::metadata(&path) {
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o600);
            let _ = fs::set_permissions(&path, permissions);
        }
    }

    Ok(())
}
