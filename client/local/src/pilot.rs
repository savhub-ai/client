use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::config::get_config_dir;

/// The embedded skill markdowns shipped with the binary.
const CONFIG_SKILL_CONTENT: &str = include_str!("../../../skills/savhub-selector-editor/SKILL.md");
const CLI_SKILL_CONTENT: &str = include_str!("../../../skills/savhub-skill-manager/SKILL.md");

// ---------------------------------------------------------------------------
// Agent skill directories
// ---------------------------------------------------------------------------

/// Skill names bundled with the pilot installer.
const BUNDLED_SKILLS: &[(&str, &str)] = &[
    ("savhub-selector-editor", CONFIG_SKILL_CONTENT),
    ("savhub-skill-manager", CLI_SKILL_CONTENT),
];

/// Return the skill installation directory for a given agent and skill name.
fn agent_skill_dir(agent: &str, skill_name: &str) -> Result<PathBuf> {
    let home = directories::UserDirs::new()
        .map(|u| u.home_dir().to_path_buf())
        .context("cannot determine home directory")?;

    let base = match agent {
        "claude-code" => home.join(".claude"),
        other => home.join(".agents").join(other),
    };
    Ok(base.join("skills").join(skill_name))
}

// ---------------------------------------------------------------------------
// Install / Uninstall
// ---------------------------------------------------------------------------

/// Return the shared skill directory: `~/.agents/skills/<skill_name>/`
fn shared_skill_dir(skill_name: &str) -> Result<PathBuf> {
    let home = directories::UserDirs::new()
        .map(|u| u.home_dir().to_path_buf())
        .context("cannot determine home directory")?;
    Ok(home.join(".agents").join("skills").join(skill_name))
}

/// Write the skill file into a directory, creating it if needed.
fn write_skill_to(dir: &PathBuf, content: &str) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
    let dest = dir.join("SKILL.md");
    fs::write(&dest, content).with_context(|| format!("failed to write {}", dest.display()))?;
    Ok(())
}

/// Install bundled skills (savhub-selector-editor + savhub-skill-manager) for the given agents.
///
/// Always installs to:
/// - `~/.agents/skills/<skill>/` (shared, for any agent)
/// - Agent-specific directories (e.g. `~/.claude/skills/<skill>/`)
///
/// Returns `(shared_dir, Vec<(agent_name, dir)>)` for the primary skill (savhub-selector-editor).
pub fn install(agents: &[String]) -> Result<(PathBuf, Vec<(String, PathBuf)>)> {
    let mut primary_shared = None;
    let mut primary_agent_dirs = Vec::new();

    for &(skill_name, content) in BUNDLED_SKILLS {
        let shared = shared_skill_dir(skill_name)?;
        write_skill_to(&shared, content)?;

        if primary_shared.is_none() {
            primary_shared = Some(shared.clone());
        }

        for agent in agents {
            let dir = agent_skill_dir(agent, skill_name)?;
            if dir == shared {
                continue;
            }
            write_skill_to(&dir, content)?;
            if skill_name == "savhub-selector-editor" {
                primary_agent_dirs.push((agent.clone(), dir));
            }
        }
    }

    Ok((primary_shared.unwrap(), primary_agent_dirs))
}

/// Uninstall bundled skills from shared and agent-specific directories.
///
/// Returns the list of directories that were removed.
pub fn uninstall(agents: &[String]) -> Result<Vec<PathBuf>> {
    let mut removed = Vec::new();

    for &(skill_name, _) in BUNDLED_SKILLS {
        if let Ok(shared) = shared_skill_dir(skill_name)
            && shared.exists()
        {
            fs::remove_dir_all(&shared)
                .with_context(|| format!("failed to remove {}", shared.display()))?;
            removed.push(shared);
        }

        for agent in agents {
            let dir = agent_skill_dir(agent, skill_name)?;
            if dir.exists() {
                fs::remove_dir_all(&dir)
                    .with_context(|| format!("failed to remove {}", dir.display()))?;
                removed.push(dir);
            }
        }
    }

    Ok(removed)
}

/// Check installation status for shared and agent-specific directories.
///
/// Returns a vec of `(label, installed_path_or_none)`.
pub fn status(agents: &[String]) -> Result<Vec<(String, Option<PathBuf>)>> {
    let mut result = Vec::new();

    for &(skill_name, _) in BUNDLED_SKILLS {
        if let Ok(shared) = shared_skill_dir(skill_name) {
            let skill_file = shared.join("SKILL.md");
            let label = format!("shared/{skill_name}");
            if skill_file.exists() {
                result.push((label, Some(shared)));
            } else {
                result.push((label, None));
            }
        }

        for agent in agents {
            let dir = agent_skill_dir(agent, skill_name)?;
            let skill_file = dir.join("SKILL.md");
            let label = format!("{agent}/{skill_name}");
            if skill_file.exists() {
                result.push((label, Some(dir)));
            } else {
                result.push((label, None));
            }
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
