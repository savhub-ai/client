use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::config::get_config_dir;

/// The embedded skill markdown shipped with the binary.
const SKILL_CONTENT: &str = include_str!("../../skills/savhub-pilot/SKILL.md");

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

/// Return the shared skill directory: `~/.agents/skills/savhub-pilot/`
fn shared_skill_dir() -> Result<PathBuf> {
    let home = directories::UserDirs::new()
        .map(|u| u.home_dir().to_path_buf())
        .context("cannot determine home directory")?;
    Ok(home.join(".agents").join("skills").join("savhub-pilot"))
}

/// Write the skill file into a directory, creating it if needed.
fn write_skill_to(dir: &PathBuf) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
    let dest = dir.join("SKILL.md");
    fs::write(&dest, SKILL_CONTENT)
        .with_context(|| format!("failed to write {}", dest.display()))?;
    Ok(())
}

/// Install the savhub-pilot skill for the given agents.
///
/// Always installs to:
/// - `~/.agents/skills/savhub-pilot/` (shared, for any agent)
/// - Agent-specific directories (e.g. `~/.claude/skills/savhub-pilot/`)
///
/// Returns `(shared_dir, Vec<(agent_name, dir)>)`.
pub fn install(agents: &[String]) -> Result<(PathBuf, Vec<(String, PathBuf)>)> {
    // Always install to the shared ~/.agents/skills/ directory
    let shared = shared_skill_dir()?;
    write_skill_to(&shared)?;

    let mut agent_dirs = Vec::new();
    for agent in agents {
        let dir = agent_skill_dir(agent)?;
        // Skip if it resolves to the same as the shared dir
        if dir == shared {
            continue;
        }
        write_skill_to(&dir)?;
        agent_dirs.push((agent.clone(), dir));
    }

    Ok((shared, agent_dirs))
}

/// Uninstall the savhub-pilot skill from shared and agent-specific directories.
///
/// Returns the list of directories that were removed.
pub fn uninstall(agents: &[String]) -> Result<Vec<PathBuf>> {
    let mut removed = Vec::new();

    // Remove from shared ~/.agents/skills/
    if let Ok(shared) = shared_skill_dir() {
        if shared.exists() {
            fs::remove_dir_all(&shared)
                .with_context(|| format!("failed to remove {}", shared.display()))?;
            removed.push(shared);
        }
    }

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

/// Check installation status for shared and agent-specific directories.
///
/// Returns a vec of `(label, installed_path_or_none)`.
pub fn status(agents: &[String]) -> Result<Vec<(String, Option<PathBuf>)>> {
    let mut result = Vec::new();

    // Check shared ~/.agents/skills/
    if let Ok(shared) = shared_skill_dir() {
        let skill_file = shared.join("SKILL.md");
        if skill_file.exists() {
            result.push(("shared".to_string(), Some(shared)));
        } else {
            result.push(("shared".to_string(), None));
        }
    }

    for agent in agents {
        let dir = agent_skill_dir(agent)?;
        let skill_file = dir.join("SKILL.md");
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
