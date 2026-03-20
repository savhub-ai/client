//! Registry cache backed by SQLite.
//!
//! The Savhub registry is a git repository containing JSON metadata files.
//! This module downloads the registry as a zip archive, parses all JSON files,
//! and stores them in a local SQLite database for fast querying.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::config::{SecurityLevel, get_config_dir};

// ---------------------------------------------------------------------------
// Security summary (matches server's SecuritySummary and registry JSON schema)
// ---------------------------------------------------------------------------

/// Compact security scan summary embedded in registry JSON files.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecuritySummary {
    /// Overall security status: unverified, scanning, verified, flagged, rejected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Aggregated verdict: clean, suspicious, malicious.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict: Option<String>,
    /// Machine-readable reason codes from static scanning.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reason_codes: Vec<String>,
    /// Human-readable summary of scan findings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Version of the scan engine that produced this result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_version: Option<String>,
    /// Timestamp of the most recent scan (RFC 3339).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scanned_at: Option<DateTime<Utc>>,
    /// Git commit SHA that was scanned. Clients should install this exact
    /// commit to guarantee they get the code that was verified.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scanned_commit: Option<String>,
}

// ---------------------------------------------------------------------------
// REST API base URL
// ---------------------------------------------------------------------------
//
// Priority (highest first):
//   1. ~/.savhub/config.toml or ~/.savhub/config.kdl → [rest_api] base_url
//   2. Caller-provided default

/// User-level config file (`~/.savhub/config.toml`).
#[derive(Debug, Clone, Default, Deserialize)]
struct UserConfigFile {
    #[serde(default)]
    rest_api: Option<UserRestApi>,
}

#[derive(Debug, Clone, Deserialize)]
struct UserRestApi {
    #[serde(default)]
    base_url: Option<String>,
}

fn user_config_path() -> Option<PathBuf> {
    let dir = get_config_dir().ok()?;
    let kdl = dir.join("config.kdl");
    if kdl.exists() {
        return Some(kdl);
    }
    Some(dir.join("config.toml"))
}

/// Read the REST API base URL.
///
/// Reads `~/.savhub/config.toml` / `config.kdl` for the legacy `[rest_api] base_url`
/// override. Normal runtime config should use the `registry` field in `config.toml`.
pub fn read_api_base_url() -> Option<String> {
    if let Some(path) = user_config_path() {
        if let Ok(raw) = std::fs::read_to_string(&path) {
            let cfg_result = if crate::kdl_support::is_kdl_path(&path) {
                crate::kdl_support::parse_kdl::<UserConfigFile>(&raw).ok()
            } else {
                toml::from_str::<UserConfigFile>(&raw).ok()
            };
            if let Some(cfg) = cfg_result {
                if let Some(url) = cfg
                    .rest_api
                    .and_then(|r| r.base_url)
                    .filter(|u| !u.trim().is_empty())
                {
                    return Some(url);
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Public constants
// ---------------------------------------------------------------------------

/// Default registry GitHub repo in `owner/repo` format.
pub const DEFAULT_REGISTRY_REPO: &str = "savhub-ai/registry";

// ---------------------------------------------------------------------------
// Data types (matching registry JSON schema)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GitRef {
    pub r#type: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RegistrySource {
    Git {
        url: String,
        r#ref: GitRef,
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        commit_hash: Option<String>,
    },
    Registry {
        path: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillEntryPoint {
    #[serde(default)]
    pub format: String,
    #[serde(default)]
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryRepo {
    #[serde(default)]
    pub schema_version: u32,
    /// Canonical identifier for the repository (e.g. `github.com/owner/repo`).
    #[serde(default)]
    pub sign: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Legacy field — older repo.json files may still have this.
    #[serde(default)]
    pub source: Option<RegistrySource>,
    /// Canonical git URL (new format).
    #[serde(default)]
    pub git_url: Option<String>,
    /// Pinned commit SHA (new format).
    #[serde(default)]
    pub git_rev: Option<String>,
    /// Branch name (new format).
    #[serde(default)]
    pub git_branch: Option<String>,
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(default)]
    pub verified: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegistryFlock {
    #[serde(default)]
    pub schema_version: u32,
    /// Canonical identifier for the flock (e.g. `github.com/owner/repo/flock-slug`).
    #[serde(default)]
    pub sign: String,
    #[serde(default, alias = "repo_sign")]
    pub repo: String,
    #[serde(default)]
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Subdirectory path within the git repo where skills are located.
    /// `None` means the repository root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(default)]
    pub license: String,
    /// Automated security scan results.
    #[serde(default, skip_serializing_if = "is_default_security")]
    pub security: SecuritySummary,
}

fn is_default_security(s: &SecuritySummary) -> bool {
    *s == SecuritySummary::default()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegistrySkill {
    pub slug: String,
    /// Path to the skill directory relative to the git repo root (e.g. "skills/salvo-auth").
    #[serde(default)]
    pub path: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Automated security scan results.
    #[serde(default, skip_serializing_if = "is_default_security")]
    pub security: SecuritySummary,
}

/// Internal representation for parsed skills.json data.
/// The JSON file is now a direct array of skills; repo_sign and flock_slug
/// are derived from the directory path.
#[derive(Debug, Clone)]
struct SkillsFile {
    repo_id: String,
    flock_slug: String,
    items: Vec<RegistrySkill>,
}

#[derive(Debug, Clone, Deserialize)]
struct FlockFile {
    #[serde(flatten)]
    flock: RegistryFlock,
}

#[derive(Debug, Clone, Deserialize)]
struct RepoFile {
    #[serde(flatten)]
    repo: RegistryRepo,
}

// ---------------------------------------------------------------------------
// Unified skill entry for UI consumption
// ---------------------------------------------------------------------------

/// Where the data came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataSource {
    /// Local SQLite cache (offline / not logged in).
    Local,
    /// Remote REST API (logged in, richer data).
    Remote,
}

/// A unified skill entry that both local SQLite and remote API can produce.
/// The UI renders this type regardless of data source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEntry {
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub status: String,
    pub license: String,
    pub categories: Vec<String>,
    pub keywords: Vec<String>,
    pub source: Option<RegistrySource>,
    /// Only available from remote API.
    pub stars: Option<u32>,
    /// Only available from remote API when logged in.
    pub starred_by_me: Option<bool>,
    /// Only available from remote API.
    pub downloads: Option<u64>,
    /// Owner/author handle — from API or derived from source URL.
    pub owner: Option<String>,
    /// Automated security scan results.
    #[serde(default, skip_serializing_if = "is_default_security")]
    pub security: SecuritySummary,
    /// Data source indicator.
    #[serde(skip)]
    pub data_source: Option<DataSource>,
}

impl From<RegistrySkill> for SkillEntry {
    fn from(s: RegistrySkill) -> Self {
        Self {
            slug: s.slug.clone(),
            name: s.name,
            description: s.description,
            version: s.version,
            status: s.status,
            license: s.license,
            categories: s.categories,
            keywords: s.keywords,
            source: None,
            stars: None,
            starred_by_me: None,
            downloads: None,
            owner: None,
            security: s.security,
            data_source: Some(DataSource::Local),
        }
    }
}

/// Stats returned after a sync operation.
#[derive(Debug, Clone)]
pub struct SyncStats {
    pub repos: usize,
    pub flocks: usize,
    pub skills: usize,
    pub commit_sha: String,
}

/// Information about the last sync.
#[derive(Debug, Clone)]
pub struct SyncInfo {
    pub commit_sha: String,
    pub synced_at: String,
    pub skill_count: usize,
}

/// Lightweight skill row for sqlite-backed catalog UIs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedSkillSummary {
    pub sign: String,
    pub slug: String,
    pub name: String,
    pub summary: Option<String>,
    pub version: Option<String>,
    pub owner: Option<String>,
    #[serde(default, skip_serializing_if = "is_default_security")]
    pub security: SecuritySummary,
}

// ---------------------------------------------------------------------------
// SQLite helpers
// ---------------------------------------------------------------------------

fn cache_db_path() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("registry.db"))
}

fn open_cache() -> Result<Connection> {
    let path = cache_db_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open registry cache at {}", path.display()))?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    init_schema(&conn)?;
    Ok(conn)
}

fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS repos (
            id          TEXT PRIMARY KEY,
            sign        TEXT NOT NULL DEFAULT '',
            name        TEXT NOT NULL DEFAULT '',
            description TEXT NOT NULL DEFAULT '',
            git_url     TEXT,
            git_rev     TEXT,
            git_branch  TEXT,
            visibility  TEXT,
            verified    INTEGER NOT NULL DEFAULT 0,
            data_json   TEXT NOT NULL DEFAULT '{}'
        );

        CREATE TABLE IF NOT EXISTS flocks (
            id              TEXT PRIMARY KEY,
            sign            TEXT NOT NULL DEFAULT '',
            repo_id         TEXT NOT NULL DEFAULT '',
            slug            TEXT NOT NULL,
            name            TEXT NOT NULL,
            path            TEXT,
            description     TEXT NOT NULL DEFAULT '',
            version         TEXT,
            status          TEXT NOT NULL DEFAULT 'active',
            license         TEXT NOT NULL DEFAULT '',
            source_json     TEXT NOT NULL DEFAULT '{}',
            data_json       TEXT NOT NULL DEFAULT '{}'
        );

        CREATE TABLE IF NOT EXISTS skills (
            id              TEXT PRIMARY KEY,
            sign            TEXT NOT NULL DEFAULT '',
            slug            TEXT NOT NULL,
            name            TEXT NOT NULL,
            path            TEXT NOT NULL DEFAULT '',
            summary         TEXT NOT NULL DEFAULT '',
            description     TEXT NOT NULL DEFAULT '',
            flock_id        TEXT NOT NULL DEFAULT '',
            repo_id         TEXT NOT NULL DEFAULT '',
            version         TEXT NOT NULL DEFAULT '0.0.0',
            status          TEXT NOT NULL DEFAULT 'active',
            license         TEXT NOT NULL DEFAULT '',
            categories_json TEXT NOT NULL DEFAULT '[]',
            keywords_json   TEXT NOT NULL DEFAULT '[]',
            source_json     TEXT NOT NULL DEFAULT '{}',
            entry_json      TEXT NOT NULL DEFAULT '{}',
            data_json       TEXT NOT NULL DEFAULT '{}',
            installed       INTEGER NOT NULL DEFAULT 0,
            installed_at    TEXT
        );

        CREATE TABLE IF NOT EXISTS sync_state (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_skills_slug ON skills(slug);
        CREATE INDEX IF NOT EXISTS idx_skills_flock ON skills(flock_id);
        CREATE INDEX IF NOT EXISTS idx_skills_status ON skills(status);
        CREATE INDEX IF NOT EXISTS idx_flocks_repo ON flocks(repo_id);

        ",
    )?;

    // Migrate: add security columns to skills and flocks if missing
    let has_security_status = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('skills') WHERE name = 'security_status'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0);
    if has_security_status == 0 {
        let _ = conn.execute_batch(
            "ALTER TABLE skills ADD COLUMN security_status TEXT NOT NULL DEFAULT '';
             ALTER TABLE skills ADD COLUMN security_verdict TEXT NOT NULL DEFAULT '';
             ALTER TABLE flocks ADD COLUMN security_status TEXT NOT NULL DEFAULT '';
             ALTER TABLE flocks ADD COLUMN security_verdict TEXT NOT NULL DEFAULT '';",
        );
    }
    // Migrate: add installed columns if missing (for existing databases)
    let has_installed = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('skills') WHERE name = 'installed'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0);
    if has_installed == 0 {
        let _ = conn.execute_batch(
            "ALTER TABLE skills ADD COLUMN installed INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE skills ADD COLUMN installed_at TEXT;",
        );
    }

    // Migrate: add path column to skills if missing
    let has_path = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('skills') WHERE name = 'path'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0);
    if has_path == 0 {
        let _ = conn.execute_batch("ALTER TABLE skills ADD COLUMN path TEXT NOT NULL DEFAULT '';");
    }

    // Migrate: add git_rev and git_branch columns to repos if missing
    let has_git_rev = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('repos') WHERE name = 'git_rev'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0);
    if has_git_rev == 0 {
        let _ = conn.execute_batch(
            "ALTER TABLE repos ADD COLUMN git_rev TEXT;
             ALTER TABLE repos ADD COLUMN git_branch TEXT;",
        );
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Skill install tracking
// ---------------------------------------------------------------------------

/// Path to `installed_skills.json`.
fn installed_skills_path() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("installed_skills.json"))
}

/// Installed skill entry in JSON file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledSkillEntry {
    pub slug: String,
    pub installed_at: String,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub path: String,
}

fn normalize_skill_repo_path(value: &str) -> String {
    value
        .trim()
        .replace('\\', "/")
        .trim_matches('/')
        .to_string()
}

fn upsert_installed_entry(entries: &mut Vec<InstalledSkillEntry>, entry: InstalledSkillEntry) {
    if let Some(existing) = entries
        .iter_mut()
        .find(|current| current.slug == entry.slug)
    {
        if !entry.repo.is_empty() {
            existing.repo = entry.repo;
        }
        if !entry.path.is_empty() {
            existing.path = entry.path;
        }
        if !entry.installed_at.is_empty() {
            existing.installed_at = entry.installed_at;
        }
    } else {
        entries.push(entry);
    }
}

pub fn installed_skill_local_path(entry: &InstalledSkillEntry) -> Option<PathBuf> {
    if entry.repo.is_empty() || entry.path.is_empty() {
        return None;
    }
    let repos_root = repos_dir().ok()?;
    Some(repos_root.join(&entry.repo).join(Path::new(&entry.path)))
}

/// Read `installed_skills.json`.
pub fn read_installed_skills_file() -> Result<Vec<InstalledSkillEntry>> {
    let path = installed_skills_path()?;
    let Ok(raw) = fs::read_to_string(&path) else {
        return Ok(Vec::new());
    };
    Ok(serde_json::from_str::<Vec<InstalledSkillEntry>>(&raw).unwrap_or_default())
}

/// Write `installed_skills.json`.
fn write_installed_skills_file(entries: &[InstalledSkillEntry]) -> Result<()> {
    let path = installed_skills_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(entries)?;
    fs::write(&path, format!("{json}\n"))?;
    Ok(())
}

/// Mark a skill as installed in both SQLite and `installed_skills.json`.
pub fn install_skill(slug: &str) -> Result<bool> {
    let conn = open_cache()?;
    let now = chrono::Utc::now().to_rfc3339();
    let updated = conn.execute(
        "UPDATE skills SET installed = 1, installed_at = ?1 WHERE slug = ?2 AND installed = 0",
        params![now, slug],
    )?;
    // Also write to JSON file
    let mut entries = read_installed_skills_file().unwrap_or_default();
    upsert_installed_entry(
        &mut entries,
        InstalledSkillEntry {
            slug: slug.to_string(),
            repo: String::new(),
            path: String::new(),
            installed_at: now,
        },
    );
    let _ = write_installed_skills_file(&entries);
    Ok(updated > 0)
}

/// Mark a skill as uninstalled in both SQLite and `installed_skills.json`.
pub fn uninstall_skill(slug: &str) -> Result<bool> {
    let conn = open_cache()?;
    let updated = conn.execute(
        "UPDATE skills SET installed = 0, installed_at = NULL WHERE slug = ?1 AND installed = 1",
        params![slug],
    )?;
    // Also update JSON file
    let mut entries = read_installed_skills_file().unwrap_or_default();
    entries.retain(|e| e.slug != slug);
    let _ = write_installed_skills_file(&entries);
    Ok(updated > 0)
}

/// Sync SQLite installed state from `installed_skills.json` (called on startup).
pub fn sync_installed_from_json() -> Result<()> {
    let entries = read_installed_skills_file()?;
    if entries.is_empty() {
        return Ok(());
    }
    let conn = open_cache()?;
    for entry in &entries {
        conn.execute(
            "UPDATE skills SET installed = 1, installed_at = ?1 WHERE slug = ?2",
            params![entry.installed_at, entry.slug],
        )?;
    }
    let _ = write_installed_skills_file(&entries);
    Ok(())
}

/// List all installed skills.
pub fn list_installed_skills() -> Result<Vec<RegistrySkill>> {
    let conn = open_cache()?;
    let mut stmt =
        conn.prepare("SELECT data_json FROM skills WHERE installed = 1 ORDER BY name ASC")?;
    let rows = stmt.query_map([], |row| {
        let json: String = row.get(0)?;
        Ok(json)
    })?;
    let mut skills = Vec::new();
    for row in rows {
        let json = row?;
        if let Ok(skill) = serde_json::from_str::<RegistrySkill>(&json) {
            skills.push(skill);
        }
    }
    Ok(skills)
}

/// List installed skill slugs (fast, no JSON parsing).
pub fn list_installed_slugs() -> Result<Vec<String>> {
    let conn = open_cache()?;
    let mut stmt = conn.prepare("SELECT slug FROM skills WHERE installed = 1 ORDER BY slug ASC")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut slugs = Vec::new();
    for row in rows {
        slugs.push(row?);
    }
    Ok(slugs)
}

/// Check if a skill is installed.
pub fn is_skill_installed(slug: &str) -> Result<bool> {
    let conn = open_cache()?;
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM skills WHERE slug = ?1 AND installed = 1",
        params![slug],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// Count installed skills.
pub fn installed_skill_count() -> Result<usize> {
    let conn = open_cache()?;
    let count: usize = conn.query_row(
        "SELECT COUNT(*) FROM skills WHERE installed = 1",
        [],
        |row| row.get(0),
    )?;
    Ok(count)
}

// ---------------------------------------------------------------------------
// Skill install via git clone
// ---------------------------------------------------------------------------

/// Directory where repos are cloned: `~/.savhub/repos/`
fn repos_dir() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("repos"))
}

/// Derive a directory name from a git URL.
/// `https://github.com/anthropics/skills` → `github.com-anthropics-skills`
fn repo_dir_name(git_url: &str) -> String {
    let stripped = git_url
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .replace("https://", "")
        .replace("http://", "");
    stripped.replace('/', "-").replace(':', "-")
}

/// Construct a skill sign: `{repo_sign}/{skill_path}`.
///
/// Example: `make_skill_sign("github.com/salvo-rs/salvo-skills", "skills/salvo-auth")`
/// → `"github.com/salvo-rs/salvo-skills/skills/salvo-auth"`
pub fn make_skill_sign(repo_sign: &str, skill_path: &str) -> String {
    format!("{repo_sign}/{skill_path}")
}

/// Look up the repo-level source for a given skill slug.
fn get_repo_source_for_skill(sign_or_slug: &str) -> Result<Option<RegistrySource>> {
    let slug = sign_or_slug.rsplit('/').next().unwrap_or(sign_or_slug);
    let conn = open_cache()?;
    let result = conn.query_row(
        "SELECT r.data_json, r.git_url, r.git_rev, r.git_branch FROM skills s JOIN repos r ON s.repo_id = r.id WHERE s.slug = ?1 LIMIT 1",
        params![slug],
        |row| Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
        )),
    );
    match result {
        Ok((json, db_git_url, db_git_rev, db_git_branch)) => {
            // 1. Try parsing data_json for git_url
            if let Ok(repo) = serde_json::from_str::<RegistryRepo>(&json) {
                if let Some(url) = &repo.git_url {
                    let branch = repo
                        .git_branch
                        .clone()
                        .unwrap_or_else(|| "main".to_string());
                    return Ok(Some(RegistrySource::Git {
                        url: url.clone(),
                        r#ref: GitRef {
                            r#type: "branch".to_string(),
                            value: branch,
                        },
                        path: None,
                        commit_hash: repo.git_rev.clone(),
                    }));
                }
                if repo.source.is_some() {
                    return Ok(repo.source);
                }
            }
            // 2. Fall back to DB columns (git_url stored during sync)
            if let Some(url) = db_git_url.filter(|u| !u.is_empty()) {
                let branch = db_git_branch.unwrap_or_else(|| "main".to_string());
                return Ok(Some(RegistrySource::Git {
                    url,
                    r#ref: GitRef {
                        r#type: "branch".to_string(),
                        value: branch,
                    },
                    path: None,
                    commit_hash: db_git_rev,
                }));
            }
            Ok(None)
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => bail!("query failed: {e}"),
    }
}

/// Look up the repo sign and skill path from the DB.
///
/// Returns `(repo_sign, skill_path)`.
/// The skill sign is `{repo_sign}/{skill_path}`.
///
/// Note: queries by slug which may not be unique across repos.
pub fn get_skill_db_info(slug: &str) -> Option<(String, String)> {
    let conn = open_cache().ok()?;
    let result = conn.query_row(
        "SELECT repo_id, path FROM skills WHERE slug = ?1 LIMIT 1",
        params![slug],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    );
    match result {
        Ok((repo_sign, path)) if !path.is_empty() => Some((repo_sign, path)),
        _ => None,
    }
}

/// Find the actual path to a skill directory within a cloned repo.
///
/// `skill_path` is the registry path (e.g. `skills/salvo-auth`).
/// Falls back to `{slug}` if the explicit path doesn't have SKILL.md.
fn find_skill_in_repo(repo_sign: &std::path::Path, skill_path: &str) -> Option<PathBuf> {
    let has_skill_md = |dir: &std::path::Path| -> bool {
        dir.join("SKILL.md").exists() || dir.join("skill.md").exists()
    };

    // 1. Try the explicit path
    let p = repo_sign.join(skill_path);
    if has_skill_md(&p) {
        return Some(p);
    }

    // 2. Try just the last segment (slug) under skills/
    let slug = skill_path.rsplit('/').next().unwrap_or(skill_path);
    let p = repo_sign.join("skills").join(slug);
    if has_skill_md(&p) {
        return Some(p);
    }

    // 3. Try slug directly
    let p = repo_sign.join(slug);
    if has_skill_md(&p) {
        return Some(p);
    }

    None
}

/// Check if a skill slug matches any entry in a list of signs or slugs.
///
/// Each entry in `skipped` can be:
/// - A full sign: `github.com/owner/repo/path/to/skill`
/// - A partial sign suffix: `path/to/skill`
/// - A plain slug: `skill-name`
/// Check if a skill matches any entry in a skipped list.
///
/// Each `skipped` entry can be:
/// - A plain slug: `salvo-auth` (matches by slug)
/// - A full sign: `github.com/salvo-rs/salvo-skills/skills/salvo-auth` (matches by sign)
/// - A partial path suffix: `skills/salvo-auth` (matches slug extracted from last segment)
///
/// `sign` is the skill sign (e.g. `github.com/owner/repo/skills/slug`).
pub fn skill_matches_skipped(sign: &str, skipped: &[String]) -> bool {
    if skipped.is_empty() {
        return false;
    }
    let slug = sign.rsplit('/').next().unwrap_or(sign);
    for entry in skipped {
        // Exact slug match
        if entry == slug {
            return true;
        }
        // Sign match
        if entry == sign {
            return true;
        }
        // Entry's last segment matches slug
        if let Some(last) = entry.rsplit('/').next() {
            if last == slug {
                return true;
            }
        }
    }
    false
}

#[derive(Debug, Clone)]
struct SkillInstallRef {
    fetch_ref: String,
    display_ref: String,
}

fn configured_security_level() -> SecurityLevel {
    crate::config::read_global_config()
        .ok()
        .flatten()
        .map(|cfg| cfg.security_level)
        .unwrap_or_default()
}

fn format_security_state(status: Option<&str>, verdict: Option<&str>) -> String {
    match (status, verdict) {
        (Some(status), Some(verdict)) if !verdict.is_empty() => {
            format!("status={status}, verdict={verdict}")
        }
        (Some(status), _) => format!("status={status}"),
        (_, Some(verdict)) if !verdict.is_empty() => format!("verdict={verdict}"),
        _ => "status=unverified".to_string(),
    }
}

fn resolve_skill_install_ref(
    skill: &RegistrySkill,
    git_ref: &GitRef,
    commit_hash: Option<&str>,
) -> Result<SkillInstallRef> {
    let level = configured_security_level();
    let status = skill.security.status.as_deref().filter(|s| !s.is_empty());
    let verdict = skill.security.verdict.as_deref().filter(|v| !v.is_empty());

    if !level.allows(status, verdict) {
        bail!(
            "skill '{}' is blocked by Security Level '{}' ({})",
            skill.slug,
            level.as_str(),
            format_security_state(status, verdict)
        );
    }

    if let Some(commit) = skill
        .security
        .scanned_commit
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        return Ok(SkillInstallRef {
            fetch_ref: commit.to_string(),
            display_ref: format!("scanned commit {commit}"),
        });
    }

    if level == SecurityLevel::Verified {
        bail!(
            "skill '{}' has no scanned commit - it has not been security-verified and cannot be installed",
            skill.slug
        );
    }

    if let Some(commit) = commit_hash.filter(|value| !value.trim().is_empty()) {
        return Ok(SkillInstallRef {
            fetch_ref: commit.to_string(),
            display_ref: format!("repo commit {commit}"),
        });
    }

    let ref_value = git_ref.value.trim();
    if ref_value.is_empty() {
        bail!(
            "skill '{}' has no scanned commit and no source ref to install",
            skill.slug
        );
    }

    Ok(SkillInstallRef {
        fetch_ref: ref_value.to_string(),
        display_ref: format!("source {} {}", git_ref.r#type, ref_value),
    })
}

fn fetch_repo_ref(repo_dir: &Path, git_url: &str, install_ref: &SkillInstallRef) -> Result<()> {
    let fetch_ok = Command::new("git")
        .args(["fetch", "--depth", "1", "origin", &install_ref.fetch_ref])
        .current_dir(repo_dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    if !fetch_ok {
        bail!(
            "failed to fetch {} from {}",
            install_ref.display_ref,
            git_url
        );
    }

    let checkout_ok = Command::new("git")
        .args(["checkout", "FETCH_HEAD"])
        .current_dir(repo_dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    if !checkout_ok {
        bail!("failed to checkout {}", install_ref.display_ref);
    }

    Ok(())
}

/// Install a skill by cloning/updating its source git repository.
///
/// 1. If the repo doesn't exist locally, `git clone --depth 1`
/// 2. If it exists, `git pull`
/// 3. Mark the skill as installed in SQLite
///
/// Returns the local path to the skill's subdirectory inside the repo.
pub fn install_skill_from_registry(sign: &str) -> Result<PathBuf> {
    let skill = get_skill_by_sign(sign)?
        .with_context(|| format!("skill '{sign}' not found in registry cache"))?;
    let slug = skill.slug.as_str();

    let source = get_repo_source_for_skill(slug)?
        .with_context(|| format!("no repo source found for skill '{slug}'"))?;
    let (git_url, git_ref, source_path, commit_hash) = match &source {
        RegistrySource::Git {
            url,
            r#ref,
            commit_hash,
            ..
        } => {
            let sp = if !skill.path.is_empty() {
                skill.path.clone()
            } else {
                format!("skills/{slug}")
            };
            (url.clone(), r#ref.clone(), sp, commit_hash.clone())
        }
        _ => bail!("skill '{slug}' has no git source"),
    };
    let install_ref = resolve_skill_install_ref(&skill, &git_ref, commit_hash.as_deref())?;

    let base = repos_dir()?;
    fs::create_dir_all(&base)?;

    let repo_name = repo_dir_name(&git_url);
    let repo_dir = base.join(&repo_name);

    if repo_dir.exists() {
        // Existing repo: fetch the exact scanned commit and checkout
        sparse_checkout_add(&repo_dir, &[source_path.as_str()])?;
        fetch_repo_ref(&repo_dir, &git_url, &install_ref)?;
    } else {
        // Fresh init + fetch of the exact scanned commit (never pulls HEAD)
        fs::create_dir_all(&repo_dir)?;

        let init_ok = std::process::Command::new("git")
            .args(["init"])
            .current_dir(&repo_dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !init_ok {
            bail!("git init failed for {git_url}");
        }

        let _ = std::process::Command::new("git")
            .args(["remote", "add", "origin", &git_url])
            .current_dir(&repo_dir)
            .status();

        // Configure sparse-checkout before fetching so only needed files are
        // downloaded.
        let _ = std::process::Command::new("git")
            .args(["sparse-checkout", "init", "--cone"])
            .current_dir(&repo_dir)
            .status();
        let _ = std::process::Command::new("git")
            .args(["sparse-checkout", "set", &source_path])
            .current_dir(&repo_dir)
            .status();

        if let Err(err) = fetch_repo_ref(&repo_dir, &git_url, &install_ref) {
            // Clean up the empty repo dir on failure
            let _ = fs::remove_dir_all(&repo_dir);
            return Err(err);
        }
    }

    // Mark as installed in SQLite + JSON
    install_skill(slug)?;

    // Find the actual skill directory within the repo
    let skill_path =
        find_skill_in_repo(&repo_dir, &source_path).unwrap_or_else(|| repo_dir.join(&source_path));

    // Update repo/path metadata in installed_skills.json
    let mut entries = read_installed_skills_file().unwrap_or_default();
    upsert_installed_entry(
        &mut entries,
        InstalledSkillEntry {
            slug: slug.to_string(),
            repo: repo_name,
            path: normalize_skill_repo_path(&source_path),
            installed_at: chrono::Utc::now().to_rfc3339(),
        },
    );
    let _ = write_installed_skills_file(&entries);

    Ok(skill_path)
}

/// Add one or more paths to the sparse-checkout list of an existing repo.
fn sparse_checkout_add(repo_sign: &std::path::Path, paths: &[&str]) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let mut args: Vec<&str> = vec!["sparse-checkout", "add"];
    args.extend(paths);
    let _ = std::process::Command::new("git")
        .args(&args)
        .current_dir(repo_sign)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .status();
    Ok(())
}

/// Info about an installed skill returned by batch install.
#[derive(Debug, Clone)]
pub struct InstalledSkillInfo {
    pub slug: String,
    /// Registry data repo path (e.g. `github.com/salvo-rs/salvo-skills`).
    pub repo_sign: String,
    /// Skill path in repo (e.g. `skills/salvo-auth`).
    pub skill_path: String,
    /// Local filesystem path to the skill directory in the cloned repo.
    pub local_path: PathBuf,
}

/// Install multiple skills in batch, grouping by git repo to minimize git operations.
///
/// Accepts slugs or signs. Slugs are looked up by slug; signs by sign.
pub fn install_skills_batch(slugs: &[String]) -> Result<Vec<InstalledSkillInfo>> {
    use std::collections::BTreeMap;

    if slugs.is_empty() {
        return Ok(Vec::new());
    }

    // 1. Resolve all skills from SQLite and extract their git source info (from repo).
    struct SkillInfo {
        slug: String,
        repo_sign: String,
        git_url: String,
        source_path: String,
        install_ref: SkillInstallRef,
    }
    let mut skill_infos = Vec::new();
    for slug in slugs {
        let skill = match get_skill_by_slug(slug)? {
            Some(s) => s,
            None => {
                eprintln!("  \x1b[33m!\x1b[0m {slug}: not found in registry cache");
                continue;
            }
        };
        // Get repo_sign from DB (repo_id column)
        let skill_repo_sign = {
            let conn = open_cache()?;
            conn.query_row(
                "SELECT repo_id FROM skills WHERE slug = ?1 LIMIT 1",
                params![slug],
                |row| row.get::<_, String>(0),
            )
            .unwrap_or_default()
        };
        let source = get_repo_source_for_skill(slug)?;
        match &source {
            Some(RegistrySource::Git {
                url,
                r#ref,
                commit_hash,
                ..
            }) => {
                let source_path = if !skill.path.is_empty() {
                    skill.path.clone()
                } else {
                    format!("skills/{slug}")
                };
                let install_ref =
                    match resolve_skill_install_ref(&skill, r#ref, commit_hash.as_deref()) {
                        Ok(install_ref) => install_ref,
                        Err(err) => {
                            eprintln!("  \x1b[33m!\x1b[0m {slug}: {err}");
                            continue;
                        }
                    };
                skill_infos.push(SkillInfo {
                    slug: slug.clone(),
                    repo_sign: skill_repo_sign,
                    git_url: url.clone(),
                    source_path,
                    install_ref,
                });
            }
            _ => {
                eprintln!("  \x1b[33m!\x1b[0m {slug}: no git source");
            }
        }
    }

    // 2. Group by repo URL and install ref so we do one git operation per exact target.
    let mut groups: BTreeMap<(String, String), Vec<&SkillInfo>> = BTreeMap::new();
    for info in &skill_infos {
        groups
            .entry((info.git_url.clone(), info.install_ref.fetch_ref.clone()))
            .or_default()
            .push(info);
    }

    let base = repos_dir()?;
    fs::create_dir_all(&base)?;

    let mut results = Vec::new();

    // 3. For each group, clone once (or pull once), sparse-checkout ALL paths at once.
    for ((git_url, _fetch_ref), skills) in &groups {
        let repo_name = repo_dir_name(git_url);
        let repo_sign = base.join(&repo_name);
        let install_ref = &skills[0].install_ref;

        let source_paths: Vec<&str> = skills.iter().map(|s| s.source_path.as_str()).collect();

        if repo_sign.exists() {
            // Existing repo: add all skill paths, then fetch the exact target ref.
            sparse_checkout_add(&repo_sign, &source_paths)?;
            if let Err(err) = fetch_repo_ref(&repo_sign, git_url, install_ref) {
                eprintln!("  \x1b[33m!\x1b[0m {repo_name}: {err}");
                continue;
            }
        } else {
            fs::create_dir_all(&repo_sign)?;

            let init_ok = Command::new("git")
                .args(["init"])
                .current_dir(&repo_sign)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|status| status.success())
                .unwrap_or(false);
            if !init_ok {
                eprintln!("  \x1b[33m!\x1b[0m git init failed for {git_url}");
                let _ = fs::remove_dir_all(&repo_sign);
                continue;
            }

            let _ = Command::new("git")
                .args(["remote", "add", "origin", git_url])
                .current_dir(&repo_sign)
                .status();

            let _ = Command::new("git")
                .args(["sparse-checkout", "init", "--cone"])
                .current_dir(&repo_sign)
                .status();

            let mut set_args: Vec<&str> = vec!["sparse-checkout", "set"];
            set_args.extend(source_paths.iter());
            let _ = Command::new("git")
                .args(&set_args)
                .current_dir(&repo_sign)
                .status();

            if let Err(err) = fetch_repo_ref(&repo_sign, git_url, install_ref) {
                eprintln!("  \x1b[33m!\x1b[0m {repo_name}: {err}");
                let _ = fs::remove_dir_all(&repo_sign);
                continue;
            }
        }

        // 4. Mark all skills in this group as installed and collect results.
        let mut entries = read_installed_skills_file().unwrap_or_default();
        for info in skills {
            let _ = install_skill(&info.slug);
            let skill_path = find_skill_in_repo(&repo_sign, &info.source_path)
                .unwrap_or_else(|| repo_sign.join(&info.source_path));
            upsert_installed_entry(
                &mut entries,
                InstalledSkillEntry {
                    slug: info.slug.clone(),
                    repo: repo_name.clone(),
                    path: normalize_skill_repo_path(&info.source_path),
                    installed_at: chrono::Utc::now().to_rfc3339(),
                },
            );
            results.push(InstalledSkillInfo {
                slug: info.slug.clone(),
                repo_sign: info.repo_sign.clone(),
                skill_path: info.source_path.clone(),
                local_path: skill_path,
            });
        }
        let _ = write_installed_skills_file(&entries);
    }

    Ok(results)
}

/// Uninstall a skill (only removes the installed mark, does not delete the repo).
pub fn uninstall_skill_from_registry(slug: &str) -> Result<()> {
    uninstall_skill(slug)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Local registry clone
// ---------------------------------------------------------------------------

/// Directory where the registry repo is cloned: `~/.savhub/registry/`
fn registry_clone_dir() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("registry"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RegistryDataPath {
    Repo { repo_id: String },
    Flock { repo_id: String, flock_slug: String },
    Skills { repo_id: String, flock_slug: String },
}

#[derive(Debug, Default)]
struct IncrementalRegistryPlan {
    repo_deletes: std::collections::BTreeSet<String>,
    repo_upserts: std::collections::BTreeSet<String>,
    flock_deletes: std::collections::BTreeSet<String>,
    flock_upserts: std::collections::BTreeSet<String>,
    skills_deletes: std::collections::BTreeSet<String>,
    skills_upserts: std::collections::BTreeSet<String>,
}

impl IncrementalRegistryPlan {
    fn total_changes(&self) -> usize {
        self.repo_deletes.len()
            + self.repo_upserts.len()
            + self.flock_deletes.len()
            + self.flock_upserts.len()
            + self.skills_deletes.len()
            + self.skills_upserts.len()
    }
}

const MAX_INCREMENTAL_PLAN_ITEMS: usize = 5000;
const MAX_REGISTRY_PARSE_WORKERS: usize = 8;

fn effective_synced_commit() -> Result<Option<String>> {
    cached_commit_sha().map(|commit| commit.filter(|value| !value.trim().is_empty()))
}

fn short_commit_sha(sha: &str) -> &str {
    &sha[..8.min(sha.len())]
}

fn git_output(args: &[&str], cwd: Option<&Path>) -> Result<String> {
    let mut command = Command::new("git");
    command.args(args);
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }
    let output = command
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            bail!("git {} failed", args.join(" "));
        }
        bail!("git {} failed: {stderr}", args.join(" "));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn git_ok(args: &[&str], cwd: Option<&Path>) -> Result<()> {
    let _ = git_output(args, cwd)?;
    Ok(())
}

fn ensure_registry_clone() -> Result<PathBuf> {
    let repo_dir = registry_clone_dir()?;
    if repo_dir.join(".git").exists() {
        return Ok(repo_dir);
    }

    if let Some(parent) = repo_dir.parent() {
        fs::create_dir_all(parent)?;
    }

    let git_url = format!("https://github.com/{DEFAULT_REGISTRY_REPO}.git");
    let target = repo_dir.display().to_string();
    git_ok(
        &[
            "clone", "--depth", "1", "--branch", "main", &git_url, &target,
        ],
        None,
    )?;
    Ok(repo_dir)
}

fn remote_registry_head(repo_dir: Option<&Path>) -> Result<String> {
    let output = if let Some(dir) = repo_dir.filter(|dir| dir.join(".git").exists()) {
        git_output(&["ls-remote", "origin", "refs/heads/main"], Some(dir))?
    } else {
        let git_url = format!("https://github.com/{DEFAULT_REGISTRY_REPO}.git");
        git_output(&["ls-remote", &git_url, "refs/heads/main"], None)?
    };

    let sha = output
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    if sha.is_empty() {
        bail!("failed to read remote registry head");
    }
    Ok(sha)
}

fn fetch_registry_main(repo_dir: &Path) -> Result<()> {
    git_ok(&["fetch", "--depth", "1", "origin", "main"], Some(repo_dir))
}

fn checkout_registry_commit(repo_dir: &Path, commit_sha: &str) -> Result<()> {
    git_ok(&["checkout", "--detach", commit_sha], Some(repo_dir))
}

fn registry_commit_exists(repo_dir: &Path, commit_sha: &str) -> bool {
    if commit_sha.trim().is_empty() {
        return false;
    }
    Command::new("git")
        .args(["cat-file", "-e", &format!("{commit_sha}^{{commit}}")])
        .current_dir(repo_dir)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn parse_registry_data_path(rel_path: &str) -> Option<RegistryDataPath> {
    let rel = rel_path.strip_prefix("data/")?;
    if let Some(repo_id) = rel.strip_suffix("/repo.json") {
        return Some(RegistryDataPath::Repo {
            repo_id: repo_id.to_string(),
        });
    }

    if let Some(stripped) = rel.strip_suffix("/flock.json") {
        let (repo_id, flock_slug) = match stripped.rsplitn(2, '/').collect::<Vec<_>>().as_slice() {
            [flock, repo] => (repo.to_string(), flock.to_string()),
            _ => return None,
        };
        return Some(RegistryDataPath::Flock {
            repo_id,
            flock_slug,
        });
    }

    if let Some(stripped) = rel.strip_suffix("/skills.json") {
        let (repo_id, flock_slug) = match stripped.rsplitn(2, '/').collect::<Vec<_>>().as_slice() {
            [flock, repo] => (repo.to_string(), flock.to_string()),
            _ => return None,
        };
        return Some(RegistryDataPath::Skills {
            repo_id,
            flock_slug,
        });
    }

    None
}

fn queue_registry_delete(plan: &mut IncrementalRegistryPlan, rel_path: &str) {
    match parse_registry_data_path(rel_path) {
        Some(RegistryDataPath::Repo { repo_id }) => {
            plan.repo_deletes.insert(repo_id);
        }
        Some(RegistryDataPath::Flock {
            repo_id,
            flock_slug,
        }) => {
            plan.flock_deletes.insert(format!("{repo_id}/{flock_slug}"));
            plan.skills_deletes
                .insert(format!("{repo_id}/{flock_slug}"));
        }
        Some(RegistryDataPath::Skills {
            repo_id,
            flock_slug,
        }) => {
            plan.skills_deletes
                .insert(format!("{repo_id}/{flock_slug}"));
        }
        None => {}
    }
}

fn queue_registry_upsert(plan: &mut IncrementalRegistryPlan, rel_path: &str) {
    match parse_registry_data_path(rel_path) {
        Some(RegistryDataPath::Repo { .. }) => {
            plan.repo_upserts.insert(rel_path.to_string());
        }
        Some(RegistryDataPath::Flock { .. }) => {
            plan.flock_upserts.insert(rel_path.to_string());
        }
        Some(RegistryDataPath::Skills { .. }) => {
            plan.skills_upserts.insert(rel_path.to_string());
        }
        None => {}
    }
}

fn diff_registry_changes(
    repo_dir: &Path,
    from_commit: &str,
    to_commit: &str,
) -> Result<IncrementalRegistryPlan> {
    let output = git_output(
        &[
            "diff",
            "--name-status",
            "--find-renames",
            from_commit,
            to_commit,
            "--",
            "data/",
        ],
        Some(repo_dir),
    )?;

    let mut plan = IncrementalRegistryPlan::default();
    for line in output.lines() {
        let mut parts = line.split('\t');
        let status = parts.next().unwrap_or_default();
        let status_kind = status.chars().next().unwrap_or_default();
        match status_kind {
            'A' | 'M' | 'T' => {
                if let Some(path) = parts.next() {
                    queue_registry_upsert(&mut plan, path);
                }
            }
            'D' => {
                if let Some(path) = parts.next() {
                    queue_registry_delete(&mut plan, path);
                }
            }
            'R' | 'C' => {
                let old_path = parts.next();
                let new_path = parts.next();
                if let Some(path) = old_path {
                    queue_registry_delete(&mut plan, path);
                }
                if let Some(path) = new_path {
                    queue_registry_upsert(&mut plan, path);
                }
            }
            _ => {}
        }
    }
    Ok(plan)
}

fn load_repo_from_clone(repo_dir: &Path, rel_path: &str) -> Result<(String, RegistryRepo, String)> {
    let RegistryDataPath::Repo { repo_id } = parse_registry_data_path(rel_path)
        .with_context(|| format!("invalid repo path: {rel_path}"))?
    else {
        bail!("invalid repo path: {rel_path}");
    };
    let raw = fs::read_to_string(repo_dir.join(rel_path))
        .with_context(|| format!("failed to read {}", repo_dir.join(rel_path).display()))?;
    let parsed = serde_json::from_str::<RepoFile>(&raw)
        .with_context(|| format!("invalid repo json: {rel_path}"))?;
    Ok((repo_id, parsed.repo, raw))
}

fn load_flock_from_clone(
    repo_dir: &Path,
    rel_path: &str,
) -> Result<(String, String, RegistryFlock, String)> {
    let RegistryDataPath::Flock {
        repo_id,
        flock_slug,
    } = parse_registry_data_path(rel_path)
        .with_context(|| format!("invalid flock path: {rel_path}"))?
    else {
        bail!("invalid flock path: {rel_path}");
    };
    let raw = fs::read_to_string(repo_dir.join(rel_path))
        .with_context(|| format!("failed to read {}", repo_dir.join(rel_path).display()))?;
    let parsed = serde_json::from_str::<FlockFile>(&raw)
        .with_context(|| format!("invalid flock json: {rel_path}"))?;
    let flock_id = format!("{repo_id}/{flock_slug}");
    let mut flock = parsed.flock;
    if flock.repo.is_empty() {
        flock.repo = repo_id;
    }
    Ok((flock_id, flock_slug, flock, raw))
}

fn load_skills_from_clone(repo_dir: &Path, rel_path: &str) -> Result<SkillsFile> {
    let RegistryDataPath::Skills {
        repo_id,
        flock_slug,
    } = parse_registry_data_path(rel_path)
        .with_context(|| format!("invalid skills path: {rel_path}"))?
    else {
        bail!("invalid skills path: {rel_path}");
    };
    let raw = fs::read_to_string(repo_dir.join(rel_path))
        .with_context(|| format!("failed to read {}", repo_dir.join(rel_path).display()))?;
    let items = serde_json::from_str::<Vec<RegistrySkill>>(&raw)
        .with_context(|| format!("invalid skills json: {rel_path}"))?;
    Ok(SkillsFile {
        repo_id,
        flock_slug,
        items,
    })
}

type LoadedRepoRow = (String, RegistryRepo, String);
type LoadedFlockRow = (String, String, RegistryFlock, String);
type LoadedRegistrySnapshot = (Vec<LoadedRepoRow>, Vec<LoadedFlockRow>, Vec<SkillsFile>);

#[derive(Clone, Copy)]
enum RegistryCloneFileKind {
    Repo,
    Flock,
    Skills,
}

#[derive(Clone)]
struct RegistryCloneFileEntry {
    path: PathBuf,
    rel_path: String,
    kind: RegistryCloneFileKind,
}

fn registry_parse_worker_count(item_count: usize) -> usize {
    if item_count < 64 {
        return 1;
    }
    std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(4)
        .min(MAX_REGISTRY_PARSE_WORKERS)
        .min(item_count)
}

fn parallel_load_rel_paths<T, F>(rel_paths: Vec<String>, loader: F) -> Result<Vec<T>>
where
    T: Send,
    F: Fn(&str) -> Result<T> + Sync,
{
    if rel_paths.is_empty() {
        return Ok(Vec::new());
    }

    let workers = registry_parse_worker_count(rel_paths.len());
    if workers <= 1 {
        let mut items = Vec::with_capacity(rel_paths.len());
        for rel_path in &rel_paths {
            items.push(loader(rel_path)?);
        }
        return Ok(items);
    }

    let chunk_size = rel_paths.len().div_ceil(workers);
    std::thread::scope(|scope| -> Result<Vec<T>> {
        let mut handles = Vec::new();
        for chunk in rel_paths.chunks(chunk_size) {
            let loader = &loader;
            handles.push(scope.spawn(move || -> Result<Vec<T>> {
                let mut items = Vec::with_capacity(chunk.len());
                for rel_path in chunk {
                    items.push(loader(rel_path)?);
                }
                Ok(items)
            }));
        }

        let mut items = Vec::with_capacity(rel_paths.len());
        for handle in handles {
            let mut chunk_items = handle
                .join()
                .map_err(|_| anyhow!("registry parse worker panicked"))??;
            items.append(&mut chunk_items);
        }
        Ok(items)
    })
}

fn load_repo_batch_from_clone(
    repo_dir: &Path,
    rel_paths: &std::collections::BTreeSet<String>,
) -> Result<Vec<LoadedRepoRow>> {
    parallel_load_rel_paths(rel_paths.iter().cloned().collect(), |rel_path| {
        load_repo_from_clone(repo_dir, rel_path)
    })
}

fn load_flock_batch_from_clone(
    repo_dir: &Path,
    rel_paths: &std::collections::BTreeSet<String>,
) -> Result<Vec<LoadedFlockRow>> {
    parallel_load_rel_paths(rel_paths.iter().cloned().collect(), |rel_path| {
        load_flock_from_clone(repo_dir, rel_path)
    })
}

fn load_skills_batch_from_clone(
    repo_dir: &Path,
    rel_paths: &std::collections::BTreeSet<String>,
) -> Result<Vec<SkillsFile>> {
    parallel_load_rel_paths(rel_paths.iter().cloned().collect(), |rel_path| {
        load_skills_from_clone(repo_dir, rel_path)
    })
}

fn parse_registry_clone_entry(entry: &RegistryCloneFileEntry) -> Result<LoadedRegistrySnapshot> {
    match entry.kind {
        RegistryCloneFileKind::Repo => {
            let raw = fs::read_to_string(&entry.path)
                .with_context(|| format!("failed to read {}", entry.path.display()))?;
            let parsed = serde_json::from_str::<RepoFile>(&raw)
                .with_context(|| format!("invalid repo json: {}", entry.rel_path))?;
            let repo_id = entry
                .rel_path
                .strip_suffix("/repo.json")
                .unwrap_or(&entry.rel_path)
                .to_string();
            Ok((vec![(repo_id, parsed.repo, raw)], Vec::new(), Vec::new()))
        }
        RegistryCloneFileKind::Flock => {
            let raw = fs::read_to_string(&entry.path)
                .with_context(|| format!("failed to read {}", entry.path.display()))?;
            let parsed = serde_json::from_str::<FlockFile>(&raw)
                .with_context(|| format!("invalid flock json: {}", entry.rel_path))?;
            let stripped = entry
                .rel_path
                .strip_suffix("/flock.json")
                .unwrap_or(&entry.rel_path);
            let (repo_id, flock_slug) =
                match stripped.rsplitn(2, '/').collect::<Vec<_>>().as_slice() {
                    [flock, repo] => (repo.to_string(), flock.to_string()),
                    _ => (parsed.flock.repo.clone(), String::new()),
                };
            let flock_id = format!("{}/{}", repo_id, flock_slug);
            let mut flock = parsed.flock;
            if flock.repo.is_empty() {
                flock.repo = repo_id;
            }
            Ok((
                Vec::new(),
                vec![(flock_id, flock_slug, flock, raw)],
                Vec::new(),
            ))
        }
        RegistryCloneFileKind::Skills => {
            let raw = fs::read_to_string(&entry.path)
                .with_context(|| format!("failed to read {}", entry.path.display()))?;
            let items = serde_json::from_str::<Vec<RegistrySkill>>(&raw)
                .with_context(|| format!("invalid skills json: {}", entry.rel_path))?;
            let stripped = entry
                .rel_path
                .strip_suffix("/skills.json")
                .unwrap_or(&entry.rel_path);
            let (repo_id, flock_slug) =
                match stripped.rsplitn(2, '/').collect::<Vec<_>>().as_slice() {
                    [flock, repo] => (repo.to_string(), flock.to_string()),
                    _ => (String::new(), String::new()),
                };
            Ok((
                Vec::new(),
                Vec::new(),
                vec![SkillsFile {
                    repo_id,
                    flock_slug,
                    items,
                }],
            ))
        }
    }
}

fn load_registry_snapshot_from_clone(data_dir: &Path) -> Result<LoadedRegistrySnapshot> {
    let mut entries = Vec::new();
    for entry in walkdir::WalkDir::new(data_dir)
        .max_depth(10)
        .into_iter()
        .flatten()
    {
        if entry.file_type().is_dir() {
            continue;
        }
        let path = entry.path();
        let Ok(rel) = path.strip_prefix(data_dir) else {
            continue;
        };
        let rel_path = rel.to_string_lossy().replace('\\', "/");
        let kind = match path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
        {
            "repo.json" => RegistryCloneFileKind::Repo,
            "flock.json" => RegistryCloneFileKind::Flock,
            "skills.json" => RegistryCloneFileKind::Skills,
            _ => continue,
        };
        entries.push(RegistryCloneFileEntry {
            path: path.to_path_buf(),
            rel_path,
            kind,
        });
    }

    let workers = registry_parse_worker_count(entries.len());
    println!(
        "[registry-sync] full clone scan: discovered {} registry files, parsing with {} worker(s)",
        entries.len(),
        workers
    );
    if entries.is_empty() {
        return Ok((Vec::new(), Vec::new(), Vec::new()));
    }
    if workers <= 1 {
        let mut repos = Vec::new();
        let mut flocks = Vec::new();
        let mut skills_files = Vec::new();
        for entry in &entries {
            let (mut r, mut f, mut s) = parse_registry_clone_entry(entry)?;
            repos.append(&mut r);
            flocks.append(&mut f);
            skills_files.append(&mut s);
        }
        return Ok((repos, flocks, skills_files));
    }

    let chunk_size = entries.len().div_ceil(workers);
    std::thread::scope(|scope| -> Result<LoadedRegistrySnapshot> {
        let mut handles = Vec::new();
        for chunk in entries.chunks(chunk_size) {
            handles.push(scope.spawn(move || -> Result<LoadedRegistrySnapshot> {
                let mut repos = Vec::new();
                let mut flocks = Vec::new();
                let mut skills_files = Vec::new();
                for entry in chunk {
                    let (mut r, mut f, mut s) = parse_registry_clone_entry(entry)?;
                    repos.append(&mut r);
                    flocks.append(&mut f);
                    skills_files.append(&mut s);
                }
                Ok((repos, flocks, skills_files))
            }));
        }

        let mut repos = Vec::new();
        let mut flocks = Vec::new();
        let mut skills_files = Vec::new();
        for handle in handles {
            let (mut r, mut f, mut s) = handle
                .join()
                .map_err(|_| anyhow!("registry parse worker panicked"))??;
            repos.append(&mut r);
            flocks.append(&mut f);
            skills_files.append(&mut s);
        }
        Ok((repos, flocks, skills_files))
    })
}

fn repo_source_json_from_repo(repo: &RegistryRepo) -> String {
    repo.source
        .as_ref()
        .map(|source| serde_json::to_string(source).unwrap_or_default())
        .unwrap_or_default()
}

fn load_repo_source_cache(
    conn: &Connection,
    repo_ids: &std::collections::BTreeSet<String>,
) -> Result<std::collections::HashMap<String, String>> {
    let mut cache = std::collections::HashMap::new();
    if repo_ids.is_empty() {
        return Ok(cache);
    }

    let mut stmt = conn.prepare("SELECT data_json FROM repos WHERE id = ?1 LIMIT 1")?;
    for repo_id in repo_ids {
        let data_json: Option<String> = stmt.query_row(params![repo_id], |row| row.get(0)).ok();
        let Some(data_json) = data_json else {
            continue;
        };
        let Ok(repo) = serde_json::from_str::<RegistryRepo>(&data_json) else {
            continue;
        };
        cache.insert(repo_id.clone(), repo_source_json_from_repo(&repo));
    }
    Ok(cache)
}

fn upsert_repo_row(
    stmt: &mut rusqlite::Statement<'_>,
    id: &str,
    repo: &RegistryRepo,
    raw: &str,
) -> Result<()> {
    let sign = if repo.sign.is_empty() {
        id.to_string()
    } else {
        repo.sign.clone()
    };
    let git_url = repo.git_url.as_deref().or_else(|| {
        repo.source.as_ref().and_then(|s| match s {
            RegistrySource::Git { url, .. } => Some(url.as_str()),
            _ => None,
        })
    });
    stmt.execute(params![
        id,
        sign,
        repo.name,
        repo.description,
        git_url,
        repo.git_rev,
        repo.git_branch,
        repo.visibility,
        repo.verified.unwrap_or(false) as i32,
        raw,
    ])?;
    Ok(())
}

fn upsert_flock_row(
    stmt: &mut rusqlite::Statement<'_>,
    id: &str,
    slug: &str,
    flock: &RegistryFlock,
    raw: &str,
) -> Result<()> {
    let flock_sign = if flock.sign.is_empty() {
        id.to_string()
    } else {
        flock.sign.clone()
    };
    stmt.execute(params![
        id,
        flock_sign,
        flock.repo,
        slug,
        flock.name,
        flock.description,
        flock.version,
        flock.status,
        flock.license,
        "",
        raw,
        flock.security.status.as_deref().unwrap_or(""),
        flock.security.verdict.as_deref().unwrap_or(""),
    ])?;
    Ok(())
}

fn replace_skills_for_flock(
    delete_stmt: &mut rusqlite::Statement<'_>,
    insert_stmt: &mut rusqlite::Statement<'_>,
    sf: &SkillsFile,
    repo_source: &str,
) -> Result<usize> {
    let flock_id = format!("{}/{}", sf.repo_id, sf.flock_slug);
    delete_stmt.execute(params![flock_id])?;

    let mut inserted = 0usize;
    for skill in &sf.items {
        let skill_id = format!("{}/{}/{}", sf.repo_id, sf.flock_slug, skill.slug);
        let categories = serde_json::to_string(&skill.categories).unwrap_or_default();
        let keywords = serde_json::to_string(&skill.keywords).unwrap_or_default();
        let data = serde_json::to_string(&skill).unwrap_or_default();
        insert_stmt.execute(params![
            skill_id,
            format!("{}/{}", sf.repo_id, sf.flock_slug),
            sf.repo_id,
            skill.slug,
            skill.name,
            skill.path,
            skill.description.as_deref().unwrap_or(""),
            skill.description.as_deref().unwrap_or(""),
            skill.version,
            skill.status,
            skill.license,
            categories,
            keywords,
            repo_source,
            "",
            data,
            skill.security.status.as_deref().unwrap_or(""),
            skill.security.verdict.as_deref().unwrap_or(""),
        ])?;
        inserted += 1;
    }

    Ok(inserted)
}

fn apply_incremental_registry_sync(
    repo_dir: &Path,
    plan: &IncrementalRegistryPlan,
    commit_sha: &str,
) -> Result<SyncStats> {
    const PROGRESS_STEP: usize = 500;
    println!(
        "[registry-sync] applying incremental sqlite sync to {}: repo -{} +{}, flock -{} +{}, skills -{} +{}",
        short_commit_sha(commit_sha),
        plan.repo_deletes.len(),
        plan.repo_upserts.len(),
        plan.flock_deletes.len(),
        plan.flock_upserts.len(),
        plan.skills_deletes.len(),
        plan.skills_upserts.len()
    );
    let parse_items =
        plan.repo_upserts.len() + plan.flock_upserts.len() + plan.skills_upserts.len();
    let parse_workers = registry_parse_worker_count(parse_items);
    println!(
        "[registry-sync] preloading changed registry files: items={}, workers={}",
        parse_items, parse_workers
    );
    let repo_upserts = load_repo_batch_from_clone(repo_dir, &plan.repo_upserts)?;
    let flock_upserts = load_flock_batch_from_clone(repo_dir, &plan.flock_upserts)?;
    let skills_upserts = load_skills_batch_from_clone(repo_dir, &plan.skills_upserts)?;
    println!(
        "[registry-sync] parsed incremental payload: repos={}, flocks={}, skills_files={}",
        repo_upserts.len(),
        flock_upserts.len(),
        skills_upserts.len()
    );
    let skill_repo_ids: std::collections::BTreeSet<String> =
        skills_upserts.iter().map(|sf| sf.repo_id.clone()).collect();
    let repo_upsert_ids: std::collections::BTreeSet<String> =
        repo_upserts.iter().map(|(id, ..)| id.clone()).collect();

    let conn = open_cache()?;
    let unchanged_repo_ids: std::collections::BTreeSet<String> = skill_repo_ids
        .difference(&repo_upsert_ids)
        .cloned()
        .collect();
    let mut repo_source_cache = load_repo_source_cache(&conn, &unchanged_repo_ids)?;
    let tx = conn.unchecked_transaction()?;
    let mut delete_skills_by_repo_stmt = tx.prepare("DELETE FROM skills WHERE repo_id = ?1")?;
    let mut delete_flocks_by_repo_stmt = tx.prepare("DELETE FROM flocks WHERE repo_id = ?1")?;
    let mut delete_repo_stmt = tx.prepare("DELETE FROM repos WHERE id = ?1")?;
    let mut delete_skills_by_flock_stmt = tx.prepare("DELETE FROM skills WHERE flock_id = ?1")?;
    let mut delete_flock_stmt = tx.prepare("DELETE FROM flocks WHERE id = ?1")?;
    let mut upsert_repo_stmt = tx.prepare(
        "INSERT OR REPLACE INTO repos (id, sign, name, description, git_url, git_rev, git_branch, visibility, verified, data_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )?;
    let mut upsert_flock_stmt = tx.prepare(
        "INSERT OR REPLACE INTO flocks (id, sign, repo_id, slug, name, description, version, status, license, source_json, data_json, security_status, security_verdict)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
    )?;
    let mut upsert_skill_stmt = tx.prepare(
        "INSERT OR REPLACE INTO skills (id, flock_id, repo_id, slug, name, path, summary, description, version, status, license, categories_json, keywords_json, source_json, entry_json, data_json, security_status, security_verdict)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
    )?;

    if !plan.repo_deletes.is_empty()
        || !plan.flock_deletes.is_empty()
        || !plan.skills_deletes.is_empty()
    {
        println!(
            "[registry-sync] deleting stale rows: repo={}, flock={}, skills={}",
            plan.repo_deletes.len(),
            plan.flock_deletes.len(),
            plan.skills_deletes.len()
        );
    }
    for repo_id in &plan.repo_deletes {
        delete_skills_by_repo_stmt.execute(params![repo_id])?;
        delete_flocks_by_repo_stmt.execute(params![repo_id])?;
        delete_repo_stmt.execute(params![repo_id])?;
    }

    for flock_id in &plan.flock_deletes {
        delete_skills_by_flock_stmt.execute(params![flock_id])?;
        delete_flock_stmt.execute(params![flock_id])?;
    }

    for flock_id in &plan.skills_deletes {
        delete_skills_by_flock_stmt.execute(params![flock_id])?;
    }

    let mut repo_count = 0usize;
    let repo_total = repo_upserts.len();
    for (id, repo, raw) in &repo_upserts {
        upsert_repo_row(&mut upsert_repo_stmt, &id, &repo, &raw)?;
        repo_source_cache.insert(id.clone(), repo_source_json_from_repo(&repo));
        repo_count += 1;
        if repo_count % PROGRESS_STEP == 0 || repo_count == repo_total {
            println!(
                "[registry-sync] repos progress: {}/{}",
                repo_count, repo_total
            );
        }
    }

    let mut flock_count = 0usize;
    let flock_total = flock_upserts.len();
    for (id, slug, flock, raw) in &flock_upserts {
        upsert_flock_row(&mut upsert_flock_stmt, &id, &slug, &flock, &raw)?;
        flock_count += 1;
        if flock_count % PROGRESS_STEP == 0 || flock_count == flock_total {
            println!(
                "[registry-sync] flocks progress: {}/{}",
                flock_count, flock_total
            );
        }
    }

    let mut skill_count = 0usize;
    let skills_total = skills_upserts.len();
    let mut skills_files_done = 0usize;
    for skills in &skills_upserts {
        let repo_source = repo_source_cache
            .get(&skills.repo_id)
            .cloned()
            .unwrap_or_default();
        skill_count += replace_skills_for_flock(
            &mut delete_skills_by_flock_stmt,
            &mut upsert_skill_stmt,
            &skills,
            &repo_source,
        )?;
        skills_files_done += 1;
        if skills_files_done % PROGRESS_STEP == 0 || skills_files_done == skills_total {
            println!(
                "[registry-sync] skills files progress: {}/{} (rows written={})",
                skills_files_done, skills_total, skill_count
            );
        }
    }

    let now = chrono::Utc::now().to_rfc3339();
    println!("[registry-sync] incremental sqlite writes finished; committing transaction");
    println!(
        "[registry-sync] writing sqlite sync_state commit_sha={} synced_at={}",
        short_commit_sha(commit_sha),
        now
    );
    tx.execute(
        "INSERT OR REPLACE INTO sync_state (key, value) VALUES ('commit_sha', ?1)",
        params![commit_sha],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO sync_state (key, value) VALUES ('synced_at', ?1)",
        params![now],
    )?;
    drop(delete_skills_by_repo_stmt);
    drop(delete_flocks_by_repo_stmt);
    drop(delete_repo_stmt);
    drop(delete_skills_by_flock_stmt);
    drop(delete_flock_stmt);
    drop(upsert_repo_stmt);
    drop(upsert_flock_stmt);
    drop(upsert_skill_stmt);
    tx.commit()?;

    Ok(SyncStats {
        repos: repo_count,
        flocks: flock_count,
        skills: skill_count,
        commit_sha: commit_sha.to_string(),
    })
}

/// Check whether the remote registry head differs from the currently synced sqlite commit.
pub fn registry_has_remote_updates() -> Result<bool> {
    let local_commit = effective_synced_commit()?;
    let repo_dir = registry_clone_dir().ok();
    let remote_commit = remote_registry_head(repo_dir.as_deref())?;
    let needs_sync = local_commit.as_deref() != Some(remote_commit.as_str());
    let local_display = local_commit
        .as_deref()
        .map(short_commit_sha)
        .unwrap_or("none");
    println!(
        "[registry-sync] update check: sqlite_commit={}, remote_head={}, needs_sync={}",
        local_display,
        short_commit_sha(&remote_commit),
        needs_sync
    );
    Ok(needs_sync)
}

/// Ensure the local registry clone is up-to-date and synced to SQLite.
///
/// 1. Compare the remote registry head with the last synced commit in SQLite
/// 2. If unchanged, skip the sqlite sync entirely
/// 3. If changed, fetch the remote head and sync only the changed registry files when the diff is
///    small
/// 4. For large diffs, or when incremental apply fails, fall back to a full sqlite rebuild
///
/// Returns `Ok(true)` if a sync was performed, `Ok(false)` if already current.
pub fn ensure_registry_synced() -> Result<bool> {
    let repo_dir = ensure_registry_clone()?;
    let synced_commit = effective_synced_commit()?.unwrap_or_default();
    let db_has_data = skill_count().unwrap_or(0) > 0;
    let remote_head = remote_registry_head(Some(&repo_dir))?;
    let synced_display = if synced_commit.is_empty() {
        "none".to_string()
    } else {
        short_commit_sha(&synced_commit).to_string()
    };
    println!(
        "[registry-sync] begin: db_has_data={}, sqlite_commit={}, remote_head={}, repo={}",
        db_has_data,
        synced_display,
        short_commit_sha(&remote_head),
        repo_dir.display()
    );

    if db_has_data && !synced_commit.is_empty() && synced_commit == remote_head {
        println!(
            "[registry-sync] skip: sqlite already synced to {}",
            short_commit_sha(&remote_head)
        );
        let _ = sync_installed_from_json();
        return Ok(false);
    }

    println!("[registry-sync] fetching latest registry main");
    fetch_registry_main(&repo_dir)?;

    let stats = if !db_has_data || synced_commit.is_empty() {
        println!(
            "[registry-sync] full sqlite rebuild: reason={}",
            if !db_has_data {
                "cache-empty"
            } else {
                "missing-synced-commit"
            }
        );
        checkout_registry_commit(&repo_dir, &remote_head)?;
        sync_from_local_clone()?
    } else if !registry_commit_exists(&repo_dir, &synced_commit) {
        println!(
            "[registry-sync] full sqlite rebuild: reason=missing-old-commit old={} new={}",
            short_commit_sha(&synced_commit),
            short_commit_sha(&remote_head)
        );
        checkout_registry_commit(&repo_dir, &remote_head)?;
        sync_from_local_clone()?
    } else {
        println!(
            "[registry-sync] diffing registry data: {} -> {}",
            short_commit_sha(&synced_commit),
            short_commit_sha(&remote_head)
        );
        let plan = diff_registry_changes(&repo_dir, &synced_commit, &remote_head)?;
        let total_changes = plan.total_changes();
        println!(
            "[registry-sync] incremental plan: repo -{} +{}, flock -{} +{}, skills -{} +{}, total={}",
            plan.repo_deletes.len(),
            plan.repo_upserts.len(),
            plan.flock_deletes.len(),
            plan.flock_upserts.len(),
            plan.skills_deletes.len(),
            plan.skills_upserts.len(),
            total_changes
        );
        checkout_registry_commit(&repo_dir, &remote_head)?;
        if total_changes > MAX_INCREMENTAL_PLAN_ITEMS {
            println!(
                "[registry-sync] full sqlite rebuild: reason=large-diff total_changes={} threshold={}",
                total_changes, MAX_INCREMENTAL_PLAN_ITEMS
            );
            sync_from_local_clone()?
        } else {
            match apply_incremental_registry_sync(&repo_dir, &plan, &remote_head) {
                Ok(stats) => stats,
                Err(err) => {
                    println!(
                        "[registry-sync] incremental sync failed: {}. Falling back to full rebuild.",
                        err
                    );
                    sync_from_local_clone()?
                }
            }
        }
    };

    println!(
        "[registry-sync] sqlite sync complete: commit={}, repos={}, flocks={}, skills={}",
        short_commit_sha(&stats.commit_sha),
        stats.repos,
        stats.flocks,
        stats.skills
    );
    println!("[registry-sync] refreshing installed flags from installed_skills.json");
    let _ = sync_installed_from_json();
    println!(
        "[registry-sync] finished: sqlite commit now {}",
        short_commit_sha(&stats.commit_sha)
    );
    Ok(true)
}

/// Ensure the local registry clone exists and fetch the latest main branch.
/// Returns the current remote main commit SHA after fetch.
pub fn clone_or_pull_registry() -> Result<String> {
    let repo_dir = ensure_registry_clone()?;
    fetch_registry_main(&repo_dir)?;
    let remote_head = remote_registry_head(Some(&repo_dir))?;
    checkout_registry_commit(&repo_dir, &remote_head)?;
    Ok(remote_head)
}

/// Synchronise the local SQLite cache from the local registry clone.
pub fn sync_from_local_clone() -> Result<SyncStats> {
    let repo_dir = registry_clone_dir()?;
    if !repo_dir.join("data").is_dir() {
        bail!("registry clone not found at {}", repo_dir.display());
    }

    // Read HEAD commit SHA
    let commit_sha = {
        let output = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&repo_dir)
            .output()
            .unwrap_or_else(|_| std::process::Output {
                status: std::process::ExitStatus::default(),
                stdout: b"unknown".to_vec(),
                stderr: Vec::new(),
            });
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };
    println!(
        "[registry-sync] full clone scan: reading data directory at {} for commit {}",
        repo_dir.join("data").display(),
        short_commit_sha(&commit_sha)
    );

    let data_dir = repo_dir.join("data");
    let (repos, flocks, skills_files) = load_registry_snapshot_from_clone(&data_dir)?;

    println!(
        "[registry-sync] parsed full registry snapshot: repos={}, flocks={}, skills_files={}",
        repos.len(),
        flocks.len(),
        skills_files.len()
    );
    write_parsed_to_db(&repos, &flocks, &skills_files, &commit_sha)
}

// ---------------------------------------------------------------------------
// Sync from zip bytes
// ---------------------------------------------------------------------------

/// Synchronise the local SQLite cache from a zip archive of the registry repo.
///
/// The `zip_bytes` should be the raw bytes of a GitHub zipball download.
/// `commit_sha` is stored so we can skip re-syncing when nothing changed.
pub fn sync_from_zip(zip_bytes: &[u8], commit_sha: &str) -> Result<SyncStats> {
    let cursor = std::io::Cursor::new(zip_bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).context("failed to open registry zip archive")?;

    let mut repos = Vec::new();
    let mut flocks = Vec::new();
    let mut skills_files = Vec::new();

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        if file.is_dir() {
            continue;
        }

        let path = file.name().to_string();
        // Strip the top-level directory (GitHub adds owner-repo-sha/)
        let rel = match path.find('/') {
            Some(pos) => &path[pos + 1..],
            None => continue,
        };

        if !rel.starts_with("data/") {
            continue;
        }

        if rel.ends_with("/repo.json") {
            let mut buf = String::new();
            file.read_to_string(&mut buf)?;
            if let Ok(parsed) = serde_json::from_str::<RepoFile>(&buf) {
                // Derive repo_id from path: data/{domain}/{owner}/{repo}/repo.json
                let repo_id = rel
                    .strip_prefix("data/")
                    .unwrap_or(rel)
                    .strip_suffix("/repo.json")
                    .unwrap_or(rel)
                    .to_string();
                repos.push((repo_id, parsed.repo, buf));
            }
        } else if rel.ends_with("/flock.json") {
            let mut buf = String::new();
            file.read_to_string(&mut buf)?;
            if let Ok(parsed) = serde_json::from_str::<FlockFile>(&buf) {
                // Derive flock slug and repo_id from path:
                // data/{domain}/{owner}/{repo}/{flock-slug}/flock.json
                let stripped = rel
                    .strip_prefix("data/")
                    .unwrap_or(rel)
                    .strip_suffix("/flock.json")
                    .unwrap_or("");
                let (repo_id, flock_slug) =
                    match stripped.rsplitn(2, '/').collect::<Vec<_>>().as_slice() {
                        [flock, repo] => (repo.to_string(), flock.to_string()),
                        _ => (parsed.flock.repo.clone(), String::new()),
                    };
                let flock_id = format!("{}/{}", repo_id, flock_slug);
                let mut flock = parsed.flock;
                if flock.repo.is_empty() {
                    flock.repo = repo_id;
                }
                flocks.push((flock_id, flock_slug, flock, buf));
            }
        } else if rel.ends_with("/skills.json") {
            let mut buf = String::new();
            file.read_to_string(&mut buf)?;
            if let Ok(items) = serde_json::from_str::<Vec<RegistrySkill>>(&buf) {
                // Derive repo_id and flock_slug from path:
                // data/{domain}/{owner}/{repo}/{flock-slug}/skills.json
                let stripped = rel
                    .strip_prefix("data/")
                    .unwrap_or(rel)
                    .strip_suffix("/skills.json")
                    .unwrap_or("");
                // Last segment is flock_slug, everything before is repo_id
                let (repo_id, flock_slug) =
                    match stripped.rsplitn(2, '/').collect::<Vec<_>>().as_slice() {
                        [flock, repo] => (repo.to_string(), flock.to_string()),
                        _ => (String::new(), String::new()),
                    };
                skills_files.push(SkillsFile {
                    repo_id,
                    flock_slug,
                    items,
                });
            }
        }
    }

    write_parsed_to_db(&repos, &flocks, &skills_files, commit_sha)
}

/// Shared logic: write parsed registry data into SQLite.
fn write_parsed_to_db(
    repos: &[(String, RegistryRepo, String)],
    flocks: &[(String, String, RegistryFlock, String)],
    skills_files: &[SkillsFile],
    commit_sha: &str,
) -> Result<SyncStats> {
    println!(
        "[registry-sync] writing full sqlite snapshot: commit={}, repos={}, flocks={}, skills_files={}",
        short_commit_sha(commit_sha),
        repos.len(),
        flocks.len(),
        skills_files.len()
    );
    let conn = open_cache()?;
    let tx = conn.unchecked_transaction()?;
    let repo_source_map: std::collections::HashMap<String, String> = repos
        .iter()
        .map(|(id, repo, _)| (id.clone(), repo_source_json_from_repo(repo)))
        .collect();

    // Save installed state before clearing
    let mut installed_map = std::collections::HashMap::<String, String>::new();
    {
        let mut stmt = tx.prepare("SELECT slug, installed_at FROM skills WHERE installed = 1")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            if let Ok((slug, at)) = row {
                installed_map.insert(slug, at);
            }
        }
    }

    // Clear all existing data
    tx.execute_batch("DELETE FROM skills; DELETE FROM flocks; DELETE FROM repos;")?;
    let mut upsert_repo_stmt = tx.prepare(
        "INSERT OR REPLACE INTO repos (id, sign, name, description, git_url, git_rev, git_branch, visibility, verified, data_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )?;
    let mut upsert_flock_stmt = tx.prepare(
        "INSERT OR REPLACE INTO flocks (id, sign, repo_id, slug, name, description, version, status, license, source_json, data_json, security_status, security_verdict)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
    )?;
    let mut upsert_skill_stmt = tx.prepare(
        "INSERT OR REPLACE INTO skills (id, flock_id, repo_id, slug, name, path, summary, description, version, status, license, categories_json, keywords_json, source_json, entry_json, data_json, security_status, security_verdict)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
    )?;
    let mut restore_installed_stmt =
        tx.prepare("UPDATE skills SET installed = 1, installed_at = ?1 WHERE slug = ?2")?;

    // Insert repos
    for (id, repo, raw) in repos {
        let sign = if repo.sign.is_empty() {
            id.clone()
        } else {
            repo.sign.clone()
        };
        let git_url = repo.git_url.as_deref().or_else(|| {
            repo.source.as_ref().and_then(|s| match s {
                RegistrySource::Git { url, .. } => Some(url.as_str()),
                _ => None,
            })
        });
        upsert_repo_stmt.execute(params![
            id,
            sign,
            repo.name,
            repo.description,
            git_url,
            repo.git_rev,
            repo.git_branch,
            repo.visibility,
            repo.verified.unwrap_or(false) as i32,
            raw,
        ])?;
    }

    // Insert flocks
    for (id, slug, flock, raw) in flocks {
        let flock_sign = if flock.sign.is_empty() {
            id.clone()
        } else {
            flock.sign.clone()
        };
        upsert_flock_stmt.execute(params![
            id,
            flock_sign,
            flock.repo,
            slug,
            flock.name,
            flock.description,
            flock.version,
            flock.status,
            flock.license,
            "", // source is now on repo level
            raw,
            flock.security.status.as_deref().unwrap_or(""),
            flock.security.verdict.as_deref().unwrap_or(""),
        ])?;
    }

    // Insert skills
    let mut total_skills = 0usize;
    for sf in skills_files {
        let flock_id = format!("{}/{}", sf.repo_id, sf.flock_slug);
        let repo_source = repo_source_map
            .get(&sf.repo_id)
            .cloned()
            .unwrap_or_default();
        for skill in &sf.items {
            // Derive id from path components
            let skill_id = format!("{}/{}/{}", sf.repo_id, sf.flock_slug, skill.slug);
            let categories = serde_json::to_string(&skill.categories).unwrap_or_default();
            let keywords = serde_json::to_string(&skill.keywords).unwrap_or_default();
            let data = serde_json::to_string(&skill).unwrap_or_default();

            upsert_skill_stmt.execute(params![
                skill_id,
                flock_id,
                sf.repo_id,
                skill.slug,
                skill.name,
                skill.path,
                skill.description.as_deref().unwrap_or(""),
                skill.description.as_deref().unwrap_or(""),
                skill.version,
                skill.status,
                skill.license,
                categories,
                keywords,
                repo_source,
                "",
                data,
                skill.security.status.as_deref().unwrap_or(""),
                skill.security.verdict.as_deref().unwrap_or(""),
            ])?;
            total_skills += 1;
        }
    }

    // Restore installed state
    for (slug, at) in &installed_map {
        restore_installed_stmt.execute(params![at, slug])?;
    }

    // Update sync state
    let now = chrono::Utc::now().to_rfc3339();
    println!(
        "[registry-sync] writing sqlite sync_state commit_sha={} synced_at={}",
        short_commit_sha(commit_sha),
        now
    );
    tx.execute(
        "INSERT OR REPLACE INTO sync_state (key, value) VALUES ('commit_sha', ?1)",
        params![commit_sha],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO sync_state (key, value) VALUES ('synced_at', ?1)",
        params![now],
    )?;
    drop(upsert_repo_stmt);
    drop(upsert_flock_stmt);
    drop(upsert_skill_stmt);
    drop(restore_installed_stmt);

    tx.commit()?;

    Ok(SyncStats {
        repos: repos.len(),
        flocks: flocks.len(),
        skills: total_skills,
        commit_sha: commit_sha.to_string(),
    })
}

// ---------------------------------------------------------------------------
// Query functions
// ---------------------------------------------------------------------------

/// Get the last sync info, or None if never synced.
pub fn sync_info() -> Result<Option<SyncInfo>> {
    let conn = open_cache()?;
    let sha: Option<String> = conn
        .query_row(
            "SELECT value FROM sync_state WHERE key = 'commit_sha'",
            [],
            |row| row.get(0),
        )
        .ok();
    let Some(commit_sha) = sha else {
        return Ok(None);
    };
    let synced_at: String = conn
        .query_row(
            "SELECT value FROM sync_state WHERE key = 'synced_at'",
            [],
            |row| row.get(0),
        )
        .unwrap_or_default();
    let skill_count: usize = conn
        .query_row("SELECT COUNT(*) FROM skills", [], |row| row.get(0))
        .unwrap_or(0);
    Ok(Some(SyncInfo {
        commit_sha,
        synced_at,
        skill_count,
    }))
}

/// Get the cached commit SHA, or None if never synced.
pub fn cached_commit_sha() -> Result<Option<String>> {
    let conn = open_cache()?;
    Ok(conn
        .query_row(
            "SELECT value FROM sync_state WHERE key = 'commit_sha'",
            [],
            |row| row.get(0),
        )
        .ok())
}

pub fn clear_cached_commit_sha() -> Result<()> {
    let conn = open_cache()?;
    conn.execute("DELETE FROM sync_state WHERE key = 'commit_sha'", [])?;
    conn.execute("DELETE FROM sync_state WHERE key = 'synced_at'", [])?;
    Ok(())
}

/// List skills with optional search, pagination, and status filter.
pub fn list_skills(
    query: Option<&str>,
    status_filter: Option<&str>,
    page: usize,
    page_size: usize,
) -> Result<(Vec<RegistrySkill>, usize)> {
    let conn = open_cache()?;
    let offset = page * page_size;

    let (where_clause, search_params) = build_search_clause(query, status_filter, false);

    // Count total
    let count_sql = format!("SELECT COUNT(*) FROM skills {where_clause}");
    let total: usize = {
        let mut stmt = conn.prepare(&count_sql)?;
        bind_search_params(&mut stmt, &search_params)?
    };

    // Fetch page
    let select_sql = format!(
        "SELECT data_json FROM skills {where_clause} ORDER BY name ASC LIMIT ?{} OFFSET ?{}",
        search_params.len() + 1,
        search_params.len() + 2,
    );
    let mut stmt = conn.prepare(&select_sql)?;
    let _param_count = search_params.len();
    let mut bound: Vec<Box<dyn rusqlite::types::ToSql>> = search_params
        .into_iter()
        .map(|s| Box::new(s) as Box<dyn rusqlite::types::ToSql>)
        .collect();
    bound.push(Box::new(page_size as i64));
    bound.push(Box::new(offset as i64));

    let refs: Vec<&dyn rusqlite::types::ToSql> = bound.iter().map(|b| b.as_ref()).collect();
    let rows = stmt.query_map(refs.as_slice(), |row| {
        let json: String = row.get(0)?;
        Ok(json)
    })?;

    let mut skills = Vec::new();
    for row in rows {
        let json = row?;
        if let Ok(skill) = serde_json::from_str::<RegistrySkill>(&json) {
            skills.push(skill);
        }
    }

    Ok((skills, total))
}

/// Search skills by name, slug, summary, or keywords. Returns up to `limit` results.
pub fn search_skills(query: &str, limit: usize) -> Result<Vec<RegistrySkill>> {
    let (skills, _) = list_skills(Some(query), Some("active"), 0, limit)?;
    Ok(skills)
}

/// Get a single skill by slug (returns first match — slugs may not be unique across repos).
pub fn get_skill_by_slug(slug: &str) -> Result<Option<RegistrySkill>> {
    let conn = open_cache()?;
    let result = conn.query_row(
        "SELECT data_json FROM skills WHERE slug = ?1 LIMIT 1",
        params![slug],
        |row| row.get::<_, String>(0),
    );
    match result {
        Ok(json) => Ok(serde_json::from_str(&json).ok()),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => bail!("query failed: {e}"),
    }
}

/// Get a single skill by sign.
pub fn get_skill_by_sign(sign: &str) -> Result<Option<RegistrySkill>> {
    let conn = open_cache()?;
    let result = conn.query_row(
        "SELECT data_json FROM skills WHERE repo_id || '/' || path = ?1 LIMIT 1",
        params![sign],
        |row| row.get::<_, String>(0),
    );
    match result {
        Ok(json) => Ok(serde_json::from_str(&json).ok()),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => bail!("query failed: {e}"),
    }
}

pub fn get_skill_by_path(repo_sign: &str, path: &str) -> Result<Option<RegistrySkill>> {
    let conn = open_cache()?;
    let result = conn.query_row(
        "SELECT data_json FROM skills WHERE repo_id = ?1 AND path = ?2 LIMIT 1",
        params![repo_sign, path],
        |row| {
            let json: String = row.get(0)?;
            Ok(json)
        },
    );
    match result {
        Ok(json) => Ok(serde_json::from_str(&json).ok()),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => bail!("query failed: {e}"),
    }
}

/// List all flocks.
pub fn list_flocks() -> Result<Vec<RegistryFlock>> {
    let conn = open_cache()?;
    let mut stmt = conn.prepare("SELECT slug, data_json FROM flocks ORDER BY name ASC")?;
    let rows = stmt.query_map([], |row| {
        let slug: String = row.get(0)?;
        let json: String = row.get(1)?;
        Ok((slug, json))
    })?;
    let mut flocks = Vec::new();
    for row in rows {
        let (slug, json) = row?;
        if let Ok(mut flock) = serde_json::from_str::<RegistryFlock>(&json) {
            flock.slug = slug;
            flocks.push(flock);
        }
    }
    Ok(flocks)
}

/// List flocks with sqlite pagination and optional search.
pub fn list_flocks_page(
    query: Option<&str>,
    page: usize,
    page_size: usize,
) -> Result<(Vec<RegistryFlock>, usize)> {
    let conn = open_cache()?;
    let offset = page * page_size;
    let (where_clause, search_params) = build_flock_search_clause(query);

    let count_sql = format!("SELECT COUNT(*) FROM flocks {where_clause}");
    let total: usize = {
        let mut stmt = conn.prepare(&count_sql)?;
        bind_search_params(&mut stmt, &search_params)?
    };

    let select_sql = format!(
        "SELECT slug, data_json FROM flocks {where_clause} ORDER BY name ASC LIMIT ?{} OFFSET ?{}",
        search_params.len() + 1,
        search_params.len() + 2,
    );
    let mut stmt = conn.prepare(&select_sql)?;
    let mut bound: Vec<Box<dyn rusqlite::types::ToSql>> = search_params
        .into_iter()
        .map(|value| Box::new(value) as Box<dyn rusqlite::types::ToSql>)
        .collect();
    bound.push(Box::new(page_size as i64));
    bound.push(Box::new(offset as i64));

    let refs: Vec<&dyn rusqlite::types::ToSql> = bound.iter().map(|value| value.as_ref()).collect();
    let rows = stmt.query_map(refs.as_slice(), |row| {
        let slug: String = row.get(0)?;
        let json: String = row.get(1)?;
        Ok((slug, json))
    })?;

    let mut flocks = Vec::new();
    for row in rows {
        let (slug, json) = row?;
        if let Ok(mut flock) = serde_json::from_str::<RegistryFlock>(&json) {
            flock.slug = slug;
            flocks.push(flock);
        }
    }

    Ok((flocks, total))
}

/// Get a flock by its slug.
pub fn get_flock_by_slug(slug: &str) -> Result<Option<RegistryFlock>> {
    let conn = open_cache()?;
    let mut stmt = conn.prepare("SELECT data_json FROM flocks WHERE slug = ?1 LIMIT 1")?;
    match stmt.query_row(rusqlite::params![slug], |row| row.get::<_, String>(0)) {
        Ok(json) => {
            let mut flock: RegistryFlock = match serde_json::from_str(&json) {
                Ok(f) => f,
                Err(_) => return Ok(None),
            };
            flock.slug = slug.to_string();
            Ok(Some(flock))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => bail!("query failed: {e}"),
    }
}

/// List all skills belonging to a flock (by flock slug).
pub fn list_skills_in_flock(flock_slug: &str) -> Result<Vec<RegistrySkill>> {
    let conn = open_cache()?;
    let pattern = format!("%/{flock_slug}");
    let mut stmt = conn.prepare(
        "SELECT data_json FROM skills WHERE flock_id LIKE ?1 OR flock_id = ?2 ORDER BY name ASC",
    )?;
    let rows = stmt.query_map(rusqlite::params![pattern, flock_slug], |row| {
        row.get::<_, String>(0)
    })?;
    let mut skills = Vec::new();
    for row in rows {
        let json = row?;
        if let Ok(skill) = serde_json::from_str::<RegistrySkill>(&json) {
            skills.push(skill);
        }
    }
    Ok(skills)
}

/// List skill slugs belonging to a flock.
pub fn list_flock_skill_slugs(flock_slug: &str) -> Result<Vec<String>> {
    let conn = open_cache()?;
    let pattern = format!("%/{flock_slug}");
    let mut stmt = conn.prepare(
        "SELECT slug FROM skills WHERE flock_id LIKE ?1 OR flock_id = ?2 ORDER BY slug ASC",
    )?;
    let rows = stmt.query_map(rusqlite::params![pattern, flock_slug], |row| {
        row.get::<_, String>(0)
    })?;
    let mut slugs = Vec::new();
    for row in rows {
        slugs.push(row?);
    }
    Ok(slugs)
}

/// List just the paths of skills in a flock (lightweight).
pub fn list_flock_skill_paths(flock_sign: &str) -> Result<Vec<String>> {
    let conn = open_cache()?;
    let pattern = format!("%/{flock_sign}");
    let mut stmt = conn.prepare(
        "SELECT slug FROM skills WHERE flock_id LIKE ?1 OR flock_id = ?2 ORDER BY slug ASC",
    )?;
    let rows = stmt.query_map(rusqlite::params![pattern, flock_sign], |row| {
        row.get::<_, String>(0)
    })?;
    let mut slugs = Vec::new();
    for row in rows {
        slugs.push(row?);
    }
    Ok(slugs)
}

/// Get the flock slug that a skill belongs to.
pub fn get_flock_slug_for_skill(skill_slug: &str) -> Result<Option<String>> {
    let conn = open_cache()?;
    let mut stmt = conn.prepare(
        "SELECT f.slug FROM flocks f JOIN skills s ON s.flock_id = f.id WHERE s.slug = ?1 LIMIT 1",
    )?;
    match stmt.query_row(rusqlite::params![skill_slug], |row| row.get::<_, String>(0)) {
        Ok(slug) => Ok(Some(slug)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => bail!("query failed: {e}"),
    }
}

/// List all flock signs from the registry cache.
///
/// Returns full signs like `github.com/owner/repo/flock-slug`.
pub fn list_flock_slugs() -> Result<Vec<String>> {
    let conn = open_cache()?;
    let mut stmt = conn.prepare("SELECT sign FROM flocks ORDER BY sign ASC")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut signs = Vec::new();
    for row in rows {
        signs.push(row?);
    }
    Ok(signs)
}

/// List all flock signs belonging to a repo (by repo sign/id, e.g. `github.com/owner/repo`).
///
/// Returns full flock signs like `github.com/owner/repo/flock-slug`.
pub fn list_repo_flock_signs(repo_sign: &str) -> Result<Vec<String>> {
    let conn = open_cache()?;
    let mut stmt = conn.prepare("SELECT sign FROM flocks WHERE repo_id = ?1 ORDER BY slug ASC")?;
    let rows = stmt.query_map(rusqlite::params![repo_sign], |row| row.get::<_, String>(0))?;
    let mut signs = Vec::new();
    for row in rows {
        signs.push(row?);
    }
    Ok(signs)
}

/// Check if a repo with the given sign (e.g. `github.com/owner/repo`) exists in the registry.
pub fn repo_exists_in_registry(sign: &str) -> Result<bool> {
    let conn = open_cache()?;
    let count: usize = conn.query_row(
        "SELECT COUNT(*) FROM repos WHERE id = ?1 OR sign = ?1",
        rusqlite::params![sign],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// Get total skill count from cache.
pub fn skill_count() -> Result<usize> {
    let conn = open_cache()?;
    let count: usize = conn.query_row("SELECT COUNT(*) FROM skills", [], |row| row.get(0))?;
    Ok(count)
}

fn owner_from_repo_id(repo_id: &str) -> Option<String> {
    let parts: Vec<&str> = repo_id.split('/').filter(|part| !part.is_empty()).collect();
    if parts.len() < 2 {
        return None;
    }
    Some(parts[parts.len() - 2].to_string())
}

/// Count cached skills matching the provided query and filter.
pub fn count_cached_skills(
    query: Option<&str>,
    status_filter: Option<&str>,
    installed_only: bool,
) -> Result<usize> {
    let conn = open_cache()?;
    let (where_clause, search_params) = build_search_clause(query, status_filter, installed_only);
    let count_sql = format!("SELECT COUNT(*) FROM skills {where_clause}");
    let mut stmt = conn.prepare(&count_sql)?;
    bind_search_params(&mut stmt, &search_params)
}

/// List cached skills as lightweight rows for the desktop catalog UI.
pub fn list_cached_skill_summaries(
    query: Option<&str>,
    status_filter: Option<&str>,
    installed_only: bool,
    page: usize,
    page_size: usize,
) -> Result<(Vec<CachedSkillSummary>, usize)> {
    let conn = open_cache()?;
    let offset = page * page_size;
    let (where_clause, search_params) = build_search_clause(query, status_filter, installed_only);

    let count_sql = format!("SELECT COUNT(*) FROM skills {where_clause}");
    let total: usize = {
        let mut stmt = conn.prepare(&count_sql)?;
        bind_search_params(&mut stmt, &search_params)?
    };

    let select_sql = format!(
        "SELECT repo_id, path, slug, name, summary, version, security_status, security_verdict
         FROM skills
         {where_clause}
         ORDER BY name ASC
         LIMIT ?{} OFFSET ?{}",
        search_params.len() + 1,
        search_params.len() + 2,
    );
    let mut stmt = conn.prepare(&select_sql)?;
    let mut bound: Vec<Box<dyn rusqlite::types::ToSql>> = search_params
        .into_iter()
        .map(|value| Box::new(value) as Box<dyn rusqlite::types::ToSql>)
        .collect();
    bound.push(Box::new(page_size as i64));
    bound.push(Box::new(offset as i64));

    let refs: Vec<&dyn rusqlite::types::ToSql> = bound.iter().map(|value| value.as_ref()).collect();
    let rows = stmt.query_map(refs.as_slice(), |row| {
        let repo_id: String = row.get(0)?;
        let path: String = row.get(1)?;
        let slug: String = row.get(2)?;
        let name: String = row.get(3)?;
        let summary: String = row.get(4)?;
        let version: String = row.get(5)?;
        let security_status: String = row.get(6)?;
        let security_verdict: String = row.get(7)?;
        Ok(CachedSkillSummary {
            sign: if path.is_empty() {
                slug.clone()
            } else {
                make_skill_sign(&repo_id, &path)
            },
            slug,
            name,
            summary: if summary.is_empty() {
                None
            } else {
                Some(summary)
            },
            version: if version.is_empty() {
                None
            } else {
                Some(version)
            },
            owner: owner_from_repo_id(&repo_id),
            security: SecuritySummary {
                status: if security_status.is_empty() {
                    None
                } else {
                    Some(security_status)
                },
                verdict: if security_verdict.is_empty() {
                    None
                } else {
                    Some(security_verdict)
                },
                ..SecuritySummary::default()
            },
        })
    })?;

    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }

    Ok((items, total))
}

/// List skills as unified `SkillEntry` items (from local SQLite).
pub fn list_skill_entries(
    query: Option<&str>,
    status_filter: Option<&str>,
    page: usize,
    page_size: usize,
) -> Result<(Vec<SkillEntry>, usize)> {
    let (skills, total) = list_skills(query, status_filter, page, page_size)?;
    let entries = skills.into_iter().map(SkillEntry::from).collect();
    Ok((entries, total))
}

/// Search skills and return as unified `SkillEntry` items.
pub fn search_skill_entries(query: &str, limit: usize) -> Result<Vec<SkillEntry>> {
    let skills = search_skills(query, limit)?;
    Ok(skills.into_iter().map(SkillEntry::from).collect())
}

/// Get a single skill by slug as a `SkillEntry`.
pub fn get_skill_entry_by_slug(slug: &str) -> Result<Option<SkillEntry>> {
    Ok(get_skill_by_slug(slug)?.map(SkillEntry::from))
}

/// Get all unique categories across all skills.
pub fn all_categories() -> Result<Vec<String>> {
    let conn = open_cache()?;
    let mut stmt = conn.prepare("SELECT DISTINCT categories_json FROM skills")?;
    let rows = stmt.query_map([], |row| {
        let json: String = row.get(0)?;
        Ok(json)
    })?;
    let mut cats = std::collections::BTreeSet::new();
    for row in rows {
        let json = row?;
        if let Ok(arr) = serde_json::from_str::<Vec<String>>(&json) {
            for cat in arr {
                if !cat.is_empty() {
                    cats.insert(cat);
                }
            }
        }
    }
    Ok(cats.into_iter().collect())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn build_search_clause(
    query: Option<&str>,
    status: Option<&str>,
    installed_only: bool,
) -> (String, Vec<String>) {
    let mut conditions = Vec::new();
    let mut params = Vec::new();

    if let Some(q) = query.filter(|q| !q.trim().is_empty()) {
        let like = format!("%{}%", q.trim().to_lowercase());
        let idx = params.len() + 1;
        conditions.push(format!(
            "(LOWER(name) LIKE ?{idx} OR LOWER(slug) LIKE ?{idx} OR LOWER(summary) LIKE ?{idx} OR LOWER(keywords_json) LIKE ?{idx})"
        ));
        params.push(like);
    }

    if let Some(s) = status.filter(|s| !s.trim().is_empty()) {
        let idx = params.len() + 1;
        conditions.push(format!("status = ?{idx}"));
        params.push(s.to_string());
    }

    if installed_only {
        conditions.push("installed = 1".to_string());
    }

    let clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    (clause, params)
}

fn build_flock_search_clause(query: Option<&str>) -> (String, Vec<String>) {
    let mut conditions = Vec::new();
    let mut params = Vec::new();

    if let Some(q) = query.filter(|q| !q.trim().is_empty()) {
        let like = format!("%{}%", q.trim().to_lowercase());
        let idx = params.len() + 1;
        conditions.push(format!(
            "(LOWER(name) LIKE ?{idx} OR LOWER(slug) LIKE ?{idx} OR LOWER(description) LIKE ?{idx})"
        ));
        params.push(like);
    }

    let clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    (clause, params)
}

fn bind_search_params(stmt: &mut rusqlite::Statement<'_>, params: &[String]) -> Result<usize> {
    let refs: Vec<&dyn rusqlite::types::ToSql> = params
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();
    let count: usize = stmt.query_row(refs.as_slice(), |row| row.get(0))?;
    Ok(count)
}
