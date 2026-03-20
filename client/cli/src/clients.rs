use std::path::PathBuf;

use anyhow::{Context, Result};

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct DetectedClient {
    pub name: String,
    pub kind: ClientKind,
    pub config_dir: PathBuf,
    pub skills_dir: PathBuf,
    pub installed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClientKind {
    ClaudeCode,
    Codex,
    Cursor,
    Windsurf,
    Continue,
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

    clients
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

fn home_dir() -> PathBuf {
    directories::UserDirs::new()
        .map(|d| d.home_dir().to_path_buf())
        .unwrap_or_else(|| {
            std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("."))
        })
}
