use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer, Serialize};

/// Minimum security level required for fetching skills/flocks.
///
/// Maps to the server's `SecurityStatus` enum:
/// - `verified` — Only fetch skills that passed all security scans
/// - `flagged`  — Also allow skills flagged as suspicious
/// - `any`      — Allow all skills regardless of security status (including unverified)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecurityLevel {
    /// Only fetch skills with status = "verified" and verdict = "clean".
    #[default]
    Verified,
    /// Also allow skills with status = "flagged" (suspicious patterns detected).
    Flagged,
    /// Allow all skills regardless of security status, including unverified.
    Any,
}

impl SecurityLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            SecurityLevel::Verified => "verified",
            SecurityLevel::Flagged => "flagged",
            SecurityLevel::Any => "any",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "flagged" => SecurityLevel::Flagged,
            "any" => SecurityLevel::Any,
            _ => SecurityLevel::Verified,
        }
    }

    /// Check if a skill/flock with the given security status and verdict is allowed.
    pub fn allows(&self, status: Option<&str>, verdict: Option<&str>) -> bool {
        match self {
            SecurityLevel::Any => true,
            SecurityLevel::Flagged => {
                // Allow verified, flagged, scanning, unverified — reject only "rejected"
                status.map_or(true, |s| s != "rejected")
            }
            SecurityLevel::Verified => {
                // Only allow verified + clean
                let status_ok = status.map_or(false, |s| s == "verified");
                let verdict_ok = verdict.map_or(true, |v| v == "clean");
                status_ok && verdict_ok
            }
        }
    }
}

fn is_default_security_level(level: &SecurityLevel) -> bool {
    *level == SecurityLevel::default()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GlobalConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workdir: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<String>,
    /// Minimum security level for fetching skills/flocks.
    #[serde(default, skip_serializing_if = "is_default_security_level")]
    pub security_level: SecurityLevel,
}

/// Resolve the savhub config/data directory.
///
/// Priority:
/// 1. `SAVHUB_CONFIG_DIR` environment variable
/// 2. `~/.savhub/` (default)
pub fn get_config_dir() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("SAVHUB_CONFIG_DIR") {
        return Ok(PathBuf::from(path));
    }
    let home = directories::UserDirs::new()
        .map(|u| u.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    Ok(home.join(".savhub"))
}

pub fn get_config_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("SAVHUB_CONFIG_PATH") {
        return Ok(PathBuf::from(path));
    }
    let dir = get_config_dir()?;
    let kdl_path = dir.join("config.kdl");
    if kdl_path.exists() {
        return Ok(kdl_path);
    }
    let toml_path = dir.join("config.toml");
    if toml_path.exists() {
        return Ok(toml_path);
    }
    // Default to KDL for new installations
    Ok(kdl_path)
}

pub fn read_global_config() -> Result<Option<GlobalConfig>> {
    let path = get_config_path()?;
    let Ok(raw) = fs::read_to_string(&path) else {
        return Ok(None);
    };
    let config = if crate::kdl_support::is_kdl_path(&path) {
        crate::kdl_support::parse_kdl::<GlobalConfig>(&raw).ok()
    } else {
        toml::from_str::<GlobalConfig>(&raw).ok()
    };
    Ok(config)
}

// ---------------------------------------------------------------------------
// Projects list — known project directories
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectEntry {
    pub path: String,
    #[serde(default)]
    pub added_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectsList {
    #[serde(default, deserialize_with = "deserialize_project_entries")]
    pub projects: Vec<ProjectEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ProjectEntryValue {
    Path(String),
    Entry(ProjectEntry),
}

fn deserialize_project_entries<'de, D>(
    deserializer: D,
) -> std::result::Result<Vec<ProjectEntry>, D::Error>
where
    D: Deserializer<'de>,
{
    let values = Vec::<ProjectEntryValue>::deserialize(deserializer)?;
    Ok(values
        .into_iter()
        .enumerate()
        .map(|(index, value)| match value {
            ProjectEntryValue::Path(path) => ProjectEntry {
                path,
                added_at: index as i64,
            },
            ProjectEntryValue::Entry(mut entry) => {
                if entry.added_at == 0 {
                    entry.added_at = index as i64;
                }
                entry
            }
        })
        .collect())
}

pub fn read_projects_list() -> Result<ProjectsList> {
    let path = get_config_dir()?.join("projects.json");
    let Ok(raw) = fs::read_to_string(&path) else {
        return Ok(ProjectsList::default());
    };
    serde_json::from_str(&raw)
        .with_context(|| format!("invalid projects list at {}", path.display()))
}

pub fn write_projects_list(list: &ProjectsList) -> Result<()> {
    let path = get_config_dir()?.join("projects.json");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let payload = serde_json::to_string_pretty(list)?;
    fs::write(&path, format!("{payload}\n"))?;
    Ok(())
}

pub fn add_project(path: &str) -> Result<()> {
    let mut list = read_projects_list()?;
    let normalized = path.replace('\\', "/");
    if !list
        .projects
        .iter()
        .any(|project| project.path.replace('\\', "/") == normalized)
    {
        list.projects.push(ProjectEntry {
            path: path.to_string(),
            added_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_secs() as i64)
                .unwrap_or(0),
        });
        write_projects_list(&list)?;
    }
    Ok(())
}

pub fn remove_project(path: &str) -> Result<()> {
    let mut list = read_projects_list()?;
    let normalized = path.replace('\\', "/");
    list.projects
        .retain(|project| project.path.replace('\\', "/") != normalized);
    write_projects_list(&list)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Global config
// ---------------------------------------------------------------------------

pub fn write_global_config(config: &GlobalConfig) -> Result<()> {
    let path = get_config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create config directory {}", parent.display()))?;
    }
    let payload = if crate::kdl_support::is_kdl_path(&path) {
        crate::kdl_support::to_kdl_string(config).map_err(|e| anyhow::anyhow!(e))?
    } else {
        toml::to_string_pretty(config).with_context(|| "failed to serialize config as TOML")?
    };
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

    let _ = crate::pilot::notify_config_changed();
    Ok(())
}
