use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};

use crate::config::get_config_dir;

/// The embedded skill markdown shipped with the binary.
const SKILL_CONTENT: &str = include_str!("../../skills/savhub-pilot/savhub-pilot.md");

// ---------------------------------------------------------------------------
// Agent skill directories
// ---------------------------------------------------------------------------

/// Return the skill installation directory for a given agent name.
///
/// - `claude-code` → `~/.claude/skills/savhub-pilot/`
/// - everything else → `~/.agents/<name>/skills/savhub-pilot/`
fn agent_skill_dir(agent: &str) -> Result<PathBuf> {
    let home = directories::UserDirs::new()
        .map(|u| u.home_dir().to_path_buf())
        .context("cannot determine home directory")?;

    let base = match agent {
        "claude-code" => home.join(".claude"),
        other => home.join(".agents").join(other),
    };
    Ok(base.join("skills").join("savhub-pilot"))
}

// ---------------------------------------------------------------------------
// Install / Uninstall
// ---------------------------------------------------------------------------

/// Install the savhub-pilot skill for the given agents.
///
/// Returns the list of directories where the skill was written.
pub fn install(agents: &[String]) -> Result<Vec<PathBuf>> {
    if agents.is_empty() {
        bail!("no agents specified; set `agents` in ~/.savhub/config or pass --agents");
    }

    let mut installed = Vec::new();
    for agent in agents {
        let dir = agent_skill_dir(agent)?;
        fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;

        let dest = dir.join("savhub-pilot.md");
        fs::write(&dest, SKILL_CONTENT)
            .with_context(|| format!("failed to write {}", dest.display()))?;

        installed.push(dir);
    }

    Ok(installed)
}

/// Uninstall the savhub-pilot skill for the given agents.
///
/// Returns the list of directories that were removed.
pub fn uninstall(agents: &[String]) -> Result<Vec<PathBuf>> {
    if agents.is_empty() {
        bail!("no agents specified; set `agents` in ~/.savhub/config or pass --agents");
    }

    let mut removed = Vec::new();
    for agent in agents {
        let dir = agent_skill_dir(agent)?;
        if dir.exists() {
            fs::remove_dir_all(&dir)
                .with_context(|| format!("failed to remove {}", dir.display()))?;
            removed.push(dir);
        }
    }

    Ok(removed)
}

/// Check installation status for each agent.
///
/// Returns a vec of `(agent_name, installed_path_or_none)`.
pub fn status(agents: &[String]) -> Result<Vec<(String, Option<PathBuf>)>> {
    let mut result = Vec::new();
    for agent in agents {
        let dir = agent_skill_dir(agent)?;
        let skill_file = dir.join("savhub-pilot.md");
        if skill_file.exists() {
            result.push((agent.clone(), Some(dir)));
        } else {
            result.push((agent.clone(), None));
        }
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// Config change notification
// ---------------------------------------------------------------------------

/// Touch the signal file so watchers (e.g. desktop app) know config changed.
pub fn notify_config_changed() -> Result<()> {
    let signal_path = config_changed_path()?;
    if let Some(parent) = signal_path.parent() {
        fs::create_dir_all(parent)?;
    }
    // Write current timestamp as content for easy debugging
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    fs::write(&signal_path, ts.to_string())?;
    Ok(())
}

/// Read the last config-changed timestamp (seconds since epoch), if any.
pub fn config_change_timestamp() -> Option<u64> {
    let path = config_changed_path().ok()?;
    let content = fs::read_to_string(path).ok()?;
    content.trim().parse().ok()
}

/// Path to the signal file.
pub fn config_changed_path() -> Result<PathBuf> {
    Ok(get_config_dir()?.join(".config-changed"))
}

/// Check if the signal file is newer than the given timestamp.
pub fn has_config_changed_since(last_seen: u64) -> bool {
    config_change_timestamp()
        .map(|ts| ts > last_seen)
        .unwrap_or(false)
}
