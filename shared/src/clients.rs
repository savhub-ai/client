use std::path::PathBuf;

use anyhow::{Context, Result};

#[derive(Debug, Clone, PartialEq)]
pub struct DetectedClient {
    pub name: String,
    pub kind: ClientKind,
    pub config_dir: PathBuf,
    pub skills_dir: PathBuf,
    pub installed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientKind {
    ClaudeCode,
    Codex,
    Cursor,
    Windsurf,
    Continue,
    VsCode,
}

impl ClientKind {
    /// Whether this client supports MCP `prompts` capability.
    pub fn supports_mcp_prompts(self) -> bool {
        matches!(
            self,
            ClientKind::ClaudeCode | ClientKind::Windsurf | ClientKind::VsCode
        )
    }

    /// Whether this client supports MCP at all (tools/resources).
    pub fn supports_mcp(self) -> bool {
        matches!(
            self,
            ClientKind::ClaudeCode
                | ClientKind::Cursor
                | ClientKind::Windsurf
                | ClientKind::Continue
                | ClientKind::VsCode
        )
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ClientKind::ClaudeCode => "claude-code",
            ClientKind::Codex => "codex",
            ClientKind::Cursor => "cursor",
            ClientKind::Windsurf => "windsurf",
            ClientKind::Continue => "continue",
            ClientKind::VsCode => "vscode",
        }
    }

    /// Returns the project-level skills directory relative path for this client, if supported.
    ///
    /// Currently supported:
    /// - Claude Code → `.claude/skills/`
    /// - Codex / Amp / OpenCode → `.agents/skills/`
    ///
    /// Returns `None` for clients that don't use a project-level skills directory
    /// (e.g. Cursor uses `.cursor/rules/` with `.mdc` files, which is a different format).
    pub fn project_skills_dir(self) -> Option<&'static str> {
        match self {
            ClientKind::ClaudeCode => Some(".claude/skills"),
            ClientKind::Codex => Some(".agents/skills"),
            // Cursor, Windsurf, Continue, VsCode use different rule formats,
            // not compatible with SKILL.md-based skills.
            _ => None,
        }
    }
}

pub fn detect_clients() -> Vec<DetectedClient> {
    let home = home_dir();
    let mut clients = Vec::new();

    // Claude Code
    let claude_dir = home.join(".claude");
    clients.push(DetectedClient {
        name: "Claude Code".to_string(),
        kind: ClientKind::ClaudeCode,
        config_dir: claude_dir.clone(),
        skills_dir: claude_dir.join("commands"),
        installed: claude_dir.is_dir(),
    });

    // Codex CLI (OpenAI)
    let codex_dir = home.join(".codex");
    clients.push(DetectedClient {
        name: "Codex".to_string(),
        kind: ClientKind::Codex,
        config_dir: codex_dir.clone(),
        skills_dir: codex_dir.join("skills"),
        installed: codex_dir.is_dir(),
    });

    // Cursor
    let cursor_dir = home.join(".cursor");
    clients.push(DetectedClient {
        name: "Cursor".to_string(),
        kind: ClientKind::Cursor,
        config_dir: cursor_dir.clone(),
        skills_dir: cursor_dir.join("rules"),
        installed: cursor_dir.is_dir(),
    });

    // Windsurf
    let windsurf_dir = home.join(".windsurf");
    let alt_windsurf = home.join(".codeium").join("windsurf");
    let windsurf_installed = windsurf_dir.is_dir() || alt_windsurf.is_dir();
    let windsurf_config = if windsurf_dir.is_dir() {
        windsurf_dir
    } else {
        alt_windsurf
    };
    clients.push(DetectedClient {
        name: "Windsurf".to_string(),
        kind: ClientKind::Windsurf,
        config_dir: windsurf_config.clone(),
        skills_dir: windsurf_config.join("rules"),
        installed: windsurf_installed,
    });

    // Continue
    let continue_dir = home.join(".continue");
    clients.push(DetectedClient {
        name: "Continue".to_string(),
        kind: ClientKind::Continue,
        config_dir: continue_dir.clone(),
        skills_dir: continue_dir.join("skills"),
        installed: continue_dir.is_dir(),
    });

    // VS Code
    let vscode_dir = home.join(".vscode");
    clients.push(DetectedClient {
        name: "VS Code".to_string(),
        kind: ClientKind::VsCode,
        config_dir: vscode_dir.clone(),
        skills_dir: vscode_dir.clone(),
        installed: vscode_dir.is_dir(),
    });

    clients
}

/// Return clients filtered by a user-configured agents list.
///
/// If `configured_agents` is empty, falls back to auto-detection (all installed clients).
/// If non-empty, only returns clients whose `kind.as_str()` matches an entry.
pub fn resolve_clients(configured_agents: &[String]) -> Vec<DetectedClient> {
    let mut all = detect_clients();
    if configured_agents.is_empty() {
        return all;
    }
    all.retain(|c| {
        configured_agents.iter().any(|a| a.eq_ignore_ascii_case(c.kind.as_str()))
    });
    // Mark retained clients as installed even if dir doesn't exist
    // (user explicitly chose them)
    for c in &mut all {
        c.installed = true;
    }
    all
}

/// Result of syncing skills to a single project-level AI client directory.
#[derive(Debug, Clone)]
pub struct ProjectSyncResult {
    pub client_kind: ClientKind,
    pub target_dir: PathBuf,
    pub skills_synced: usize,
}

/// Sync skills from the project's savhub skills directory to project-level AI client
/// directories (e.g. `.claude/skills/`, `.agents/skills/`).
///
/// Only syncs to clients that are detected as installed on the machine.
/// Returns a list of sync results for each client that was synced to.
pub fn sync_skills_to_project(
    workdir: &std::path::Path,
    skills_source: &std::path::Path,
) -> Result<Vec<ProjectSyncResult>> {
    let clients = detect_clients();
    sync_skills_to_project_for_clients(workdir, skills_source, &clients)
}

/// Sync skills to project-level AI client directories for specific clients.
///
/// This allows callers to filter which clients to sync to.
pub fn sync_skills_to_project_for_clients(
    workdir: &std::path::Path,
    skills_source: &std::path::Path,
    clients: &[DetectedClient],
) -> Result<Vec<ProjectSyncResult>> {
    if !skills_source.is_dir() {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();

    for client in clients {
        if !client.installed {
            continue;
        }
        let Some(rel_dir) = client.kind.project_skills_dir() else {
            continue;
        };

        let target_dir = workdir.join(rel_dir);
        std::fs::create_dir_all(&target_dir)
            .with_context(|| format!("failed to create {}", target_dir.display()))?;

        let mut count = 0;
        let entries = std::fs::read_dir(skills_source)
            .with_context(|| format!("failed to read {}", skills_source.display()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if name.starts_with('.') {
                continue;
            }
            if !path.join("SKILL.md").exists() && !path.join("skill.md").exists() {
                continue;
            }
            let target = target_dir.join(&name);
            copy_dir_recursive(&path, &target)?;
            count += 1;
        }

        if count > 0 {
            results.push(ProjectSyncResult {
                client_kind: client.kind,
                target_dir,
                skills_synced: count,
            });
        }
    }

    Ok(results)
}

/// Returns the global skills directory for savhub.
pub fn global_skills_dir() -> PathBuf {
    directories::ProjectDirs::from("", "", "savhub")
        .map(|d| d.data_dir().join("skills"))
        .unwrap_or_else(|| PathBuf::from(".savhub").join("skills"))
}

/// Sync skills from a source directory to a detected AI client.
pub fn sync_skills_to_client(
    client: &DetectedClient,
    skills_source: &std::path::Path,
) -> Result<usize> {
    if !client.installed {
        anyhow::bail!("{} is not installed", client.name);
    }
    std::fs::create_dir_all(&client.skills_dir)
        .with_context(|| format!("failed to create {}", client.skills_dir.display()))?;

    let mut count = 0;
    let entries = std::fs::read_dir(skills_source)
        .with_context(|| format!("failed to read {}", skills_source.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if name.starts_with('.') {
            continue;
        }
        if !path.join("SKILL.md").exists() && !path.join("skill.md").exists() {
            continue;
        }
        let target = client.skills_dir.join(&name);
        copy_dir_recursive(&path, &target)?;
        count += 1;
    }
    Ok(count)
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            let name = src_path.file_name().unwrap_or_default().to_string_lossy();
            if name.starts_with('.') || name.as_ref() == "node_modules" || name.as_ref() == "target"
            {
                continue;
            }
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

pub fn home_dir() -> PathBuf {
    directories::UserDirs::new()
        .map(|d| d.home_dir().to_path_buf())
        .unwrap_or_else(|| {
            std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("."))
        })
}
