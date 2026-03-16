//! Registry cache backed by SQLite.
//!
//! The Savhub registry is a git repository containing JSON metadata files.
//! This module downloads the registry as a zip archive, parses all JSON files,
//! and stores them in a local SQLite database for fast querying.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::config::get_config_dir;

// ---------------------------------------------------------------------------
// REST API base URL
// ---------------------------------------------------------------------------
//
// Priority (highest first):
//   1. ~/.savhub/config.toml  → [rest_api] base_url  (user override, for testing)
//   2. ~/.config/savhub/registry.json → rest_api.base_url (from registry)
//   3. Caller-provided default

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

/// Registry config file (`~/.config/savhub/registry.json`).
#[derive(Debug, Clone, Deserialize)]
struct RegistryConfigFile {
    #[serde(default)]
    rest_api: Option<RegistryRestApi>,
}

#[derive(Debug, Clone, Deserialize)]
struct RegistryRestApi {
    #[serde(default)]
    base_url: Option<String>,
}

fn user_config_path() -> Option<PathBuf> {
    get_config_dir().ok().map(|d| d.join("config.toml"))
}

/// Read the REST API base URL.
///
/// Checks `~/.savhub/config.toml` first (user override for testing),
/// then falls back to `~/.config/savhub/registry.json`.
pub fn read_api_base_url() -> Option<String> {
    // 1. User override: ~/.savhub/config.toml
    if let Some(path) = user_config_path() {
        if let Ok(raw) = std::fs::read_to_string(&path) {
            if let Ok(cfg) = toml::from_str::<UserConfigFile>(&raw) {
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

    // 2. Registry config: ~/.config/savhub/registry.json
    let path = get_config_dir().ok()?.join("registry.json");
    let raw = std::fs::read_to_string(&path).ok()?;
    let cfg: RegistryConfigFile = serde_json::from_str(&raw).ok()?;
    cfg.rest_api?.base_url.filter(|u| !u.trim().is_empty())
}

/// Write the REST API base URL to `~/.config/savhub/registry.json`.
pub fn write_api_base_url(base_url: &str) -> Result<()> {
    let path = get_config_dir()?.join("registry.json");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let payload = serde_json::json!({
        "version": "1.0",
        "rest_api": {
            "base_url": base_url
        }
    });
    let json = serde_json::to_string_pretty(&payload)?;
    std::fs::write(&path, format!("{json}\n"))?;
    Ok(())
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(default)]
    pub license: String,
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

/// Install a skill by cloning/updating its source git repository.
///
/// 1. If the repo doesn't exist locally, `git clone --depth 1`
/// 2. If it exists, `git pull`
/// 3. Mark the skill as installed in SQLite
///
/// Returns the local path to the skill's subdirectory inside the repo.
pub fn install_skill_from_registry(sign: &str) -> Result<PathBuf> {
    let slug = sign.rsplit('/').next().unwrap_or(sign);
    let skill = get_skill_by_sign(sign)?
        .with_context(|| format!("skill '{sign}' not found in registry cache"))?;

    let source = get_repo_source_for_skill(sign)?
        .with_context(|| format!("no repo source found for skill '{sign}'"))?;
    let (git_url, git_ref, source_path) = match &source {
        RegistrySource::Git { url, r#ref, .. } => {
            let sp = if !skill.path.is_empty() {
                skill.path.clone()
            } else {
                format!("skills/{slug}")
            };
            (url.clone(), r#ref.clone(), sp)
        }
        _ => bail!("skill '{sign}' has no git source"),
    };

    let base = repos_dir()?;
    fs::create_dir_all(&base)?;

    let repo_name = repo_dir_name(&git_url);
    let repo_sign = base.join(&repo_name);

    if repo_sign.exists() {
        // Existing repo: add the skill path to sparse-checkout and pull
        sparse_checkout_add(&repo_sign, &[source_path.as_str()])?;

        let status = std::process::Command::new("git")
            .args(["pull", "--ff-only"])
            .current_dir(&repo_sign)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .status();
        match status {
            Ok(s) if s.success() => {}
            _ => {
                // Pull failed, try fetch+reset
                let _ = std::process::Command::new("git")
                    .args(["fetch", "--depth", "1", "origin", &git_ref.value])
                    .current_dir(&repo_sign)
                    .status();
                let _ = std::process::Command::new("git")
                    .args(["checkout", &git_ref.value, "--"])
                    .current_dir(&repo_sign)
                    .status();
            }
        }
    } else {
        // Fresh shallow clone with sparse-checkout (only check out the needed path)
        let mut args = vec![
            "clone".to_string(),
            "--depth".to_string(),
            "1".to_string(),
            "--no-checkout".to_string(),
            "--filter=blob:none".to_string(),
        ];
        if git_ref.r#type == "branch" || git_ref.r#type == "tag" {
            args.push("--branch".to_string());
            args.push(git_ref.value.clone());
        }
        args.push(git_url.clone());
        args.push(repo_sign.display().to_string());

        let status = std::process::Command::new("git")
            .args(&args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .status()
            .with_context(|| "failed to run git clone")?;

        if !status.success() {
            bail!("git clone failed for {git_url}");
        }

        // Initialize sparse-checkout with the skill path
        let _ = std::process::Command::new("git")
            .args(["sparse-checkout", "init", "--cone"])
            .current_dir(&repo_sign)
            .status();
        let _ = std::process::Command::new("git")
            .args(["sparse-checkout", "set", &source_path])
            .current_dir(&repo_sign)
            .status();
        let _ = std::process::Command::new("git")
            .args(["checkout"])
            .current_dir(&repo_sign)
            .status();
    }

    // Mark as installed in SQLite + JSON
    install_skill(slug)?;

    // Find the actual skill directory within the repo
    let skill_path = find_skill_in_repo(&repo_sign, &source_path)
        .unwrap_or_else(|| repo_sign.join(&source_path));

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
        git_ref: GitRef,
        source_path: String,
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
            Some(RegistrySource::Git { url, r#ref, .. }) => {
                let source_path = if !skill.path.is_empty() {
                    skill.path.clone()
                } else {
                    format!("skills/{slug}")
                };
                skill_infos.push(SkillInfo {
                    slug: slug.clone(),
                    repo_sign: skill_repo_sign,
                    git_url: url.clone(),
                    git_ref: r#ref.clone(),
                    source_path,
                });
            }
            _ => {
                eprintln!("  \x1b[33m!\x1b[0m {slug}: no git source");
            }
        }
    }

    // 2. Group by (git_url, git_ref.value) so we do one git operation per repo.
    let mut groups: BTreeMap<(String, String), Vec<&SkillInfo>> = BTreeMap::new();
    for info in &skill_infos {
        groups
            .entry((info.git_url.clone(), info.git_ref.value.clone()))
            .or_default()
            .push(info);
    }

    let base = repos_dir()?;
    fs::create_dir_all(&base)?;

    let mut results = Vec::new();

    // 3. For each group, clone once (or pull once), sparse-checkout ALL paths at once.
    for ((git_url, _ref_value), skills) in &groups {
        let repo_name = repo_dir_name(git_url);
        let repo_sign = base.join(&repo_name);
        let git_ref = &skills[0].git_ref;

        let source_paths: Vec<&str> = skills.iter().map(|s| s.source_path.as_str()).collect();

        if repo_sign.exists() {
            // Existing repo: add ALL skill paths in a single sparse-checkout command.
            sparse_checkout_add(&repo_sign, &source_paths)?;

            let status = std::process::Command::new("git")
                .args(["pull", "--ff-only"])
                .current_dir(&repo_sign)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::piped())
                .status();
            match status {
                Ok(s) if s.success() => {}
                _ => {
                    let _ = std::process::Command::new("git")
                        .args(["fetch", "--depth", "1", "origin", &git_ref.value])
                        .current_dir(&repo_sign)
                        .status();
                    let _ = std::process::Command::new("git")
                        .args(["checkout", &git_ref.value, "--"])
                        .current_dir(&repo_sign)
                        .status();
                }
            }
        } else {
            // Fresh shallow clone with sparse-checkout for all paths at once.
            let mut args = vec![
                "clone".to_string(),
                "--depth".to_string(),
                "1".to_string(),
                "--no-checkout".to_string(),
                "--filter=blob:none".to_string(),
            ];
            if git_ref.r#type == "branch" || git_ref.r#type == "tag" {
                args.push("--branch".to_string());
                args.push(git_ref.value.clone());
            }
            args.push(git_url.clone());
            args.push(repo_sign.display().to_string());

            let status = std::process::Command::new("git")
                .args(&args)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .status()
                .with_context(|| format!("failed to run git clone for {git_url}"))?;

            if !status.success() {
                eprintln!("  \x1b[33m!\x1b[0m git clone failed for {git_url}");
                continue;
            }

            let _ = std::process::Command::new("git")
                .args(["sparse-checkout", "init", "--cone"])
                .current_dir(&repo_sign)
                .status();

            // Set ALL paths in a single sparse-checkout set command.
            let mut set_args: Vec<&str> = vec!["sparse-checkout", "set"];
            set_args.extend(source_paths.iter());
            let _ = std::process::Command::new("git")
                .args(&set_args)
                .current_dir(&repo_sign)
                .status();

            let _ = std::process::Command::new("git")
                .args(["checkout"])
                .current_dir(&repo_sign)
                .status();
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

/// Persistent state for the registry sync, stored as `~/.savhub/registry.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RegistryState {
    /// The git commit SHA that was last synced into registry.db.
    #[serde(default)]
    pub synced_commit: String,
}

fn registry_state_path() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("registry.json"))
}

pub fn read_registry_state() -> Result<RegistryState> {
    let path = registry_state_path()?;
    let Ok(raw) = fs::read_to_string(&path) else {
        return Ok(RegistryState::default());
    };
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

pub fn write_registry_state(state: &RegistryState) -> Result<()> {
    let path = registry_state_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let payload = serde_json::to_string_pretty(state)?;
    fs::write(&path, format!("{payload}\n"))?;
    Ok(())
}

/// Ensure the local registry clone is up-to-date and synced to SQLite.
///
/// 1. Clone or pull the registry repo
/// 2. Read HEAD commit SHA
/// 3. Compare with `registry.json` — skip sync if already up-to-date
/// 4. If out of date (or registry.json missing), sync to SQLite and update registry.json
///
/// Returns `Ok(true)` if a sync was performed, `Ok(false)` if already current.
pub fn ensure_registry_synced() -> Result<bool> {
    let head_sha = clone_or_pull_registry()?;
    let state = read_registry_state()?;

    if !state.synced_commit.is_empty() && state.synced_commit == head_sha {
        let _ = sync_installed_from_json();
        return Ok(false); // Already synced
    }

    sync_from_local_clone()?;

    write_registry_state(&RegistryState {
        synced_commit: head_sha,
    })?;

    // Restore installed state from installed_skills.json
    let _ = sync_installed_from_json();

    Ok(true)
}

/// Clone or pull the registry repo. Returns the current HEAD commit SHA.
pub fn clone_or_pull_registry() -> Result<String> {
    let repo_dir = registry_clone_dir()?;
    let git_url = format!("https://github.com/{DEFAULT_REGISTRY_REPO}.git");

    if repo_dir.join(".git").exists() {
        // Pull updates
        let status = std::process::Command::new("git")
            .args(["pull", "--ff-only"])
            .current_dir(&repo_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .status();
        if let Ok(s) = status {
            if !s.success() {
                // Pull failed, try fetch+reset
                let _ = std::process::Command::new("git")
                    .args(["fetch", "origin", "main"])
                    .current_dir(&repo_dir)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
                let _ = std::process::Command::new("git")
                    .args(["reset", "--hard", "origin/main"])
                    .current_dir(&repo_dir)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }
        }
    } else {
        // Fresh clone
        if let Some(parent) = repo_dir.parent() {
            fs::create_dir_all(parent)?;
        }
        let status = std::process::Command::new("git")
            .args([
                "clone",
                "--depth",
                "1",
                &git_url,
                &repo_dir.display().to_string(),
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .status()
            .context("failed to run git clone")?;
        if !status.success() {
            bail!("git clone failed for {git_url}");
        }
    }

    // Read HEAD commit SHA
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&repo_dir)
        .output()
        .context("failed to read git HEAD")?;
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(sha)
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

    let data_dir = repo_dir.join("data");
    let mut repos = Vec::new();
    let mut flocks = Vec::new();
    let mut skills_files = Vec::new();

    for entry in walkdir::WalkDir::new(&data_dir)
        .max_depth(10)
        .into_iter()
        .flatten()
    {
        if entry.file_type().is_dir() {
            continue;
        }
        let path = entry.path();
        let Ok(rel) = path.strip_prefix(&data_dir) else {
            continue;
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");

        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        match file_name {
            "repo.json" => {
                let buf = fs::read_to_string(path).unwrap_or_default();
                if let Ok(parsed) = serde_json::from_str::<RepoFile>(&buf) {
                    let repo_id = rel_str
                        .strip_suffix("/repo.json")
                        .unwrap_or(&rel_str)
                        .to_string();
                    repos.push((repo_id, parsed.repo, buf));
                }
            }
            "flock.json" => {
                let buf = fs::read_to_string(path).unwrap_or_default();
                if let Ok(parsed) = serde_json::from_str::<FlockFile>(&buf) {
                    let stripped = rel_str.strip_suffix("/flock.json").unwrap_or(&rel_str);
                    // Path: domain/owner/repo/flock_slug → repo_id = domain/owner/repo
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
            }
            "skills.json" => {
                let buf = fs::read_to_string(path).unwrap_or_default();
                if let Ok(items) = serde_json::from_str::<Vec<RegistrySkill>>(&buf) {
                    let stripped = rel_str.strip_suffix("/skills.json").unwrap_or(&rel_str);
                    // Path: domain/owner/repo/flock_slug → repo_id = domain/owner/repo
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
            _ => {}
        }
    }

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
                let (repo_id, flock_slug) = match stripped.rsplitn(2, '/').collect::<Vec<_>>().as_slice() {
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
                let (repo_id, flock_slug) = match stripped.rsplitn(2, '/').collect::<Vec<_>>().as_slice() {
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
    let conn = open_cache()?;
    let tx = conn.unchecked_transaction()?;

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

    // Insert repos
    for (id, repo, raw) in repos {
        let sign = if repo.sign.is_empty() { id.clone() } else { repo.sign.clone() };
        let git_url = repo.git_url.as_deref().or_else(|| {
            repo.source.as_ref().and_then(|s| match s {
                RegistrySource::Git { url, .. } => Some(url.as_str()),
                _ => None,
            })
        });
        tx.execute(
            "INSERT OR REPLACE INTO repos (id, sign, name, description, git_url, git_rev, git_branch, visibility, verified, data_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
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
            ],
        )?;
    }

    // Insert flocks
    for (id, slug, flock, raw) in flocks {
        let flock_sign = if flock.sign.is_empty() { id.clone() } else { flock.sign.clone() };
        tx.execute(
            "INSERT OR REPLACE INTO flocks (id, sign, repo_id, slug, name, description, version, status, license, source_json, data_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
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
            ],
        )?;
    }

    // Insert skills
    let mut total_skills = 0usize;
    for sf in skills_files {
        let flock_id = format!("{}/{}", sf.repo_id, sf.flock_slug);
        // Look up repo source for this skill's repo
        let repo_source = repos
            .iter()
            .find(|(id, _, _)| *id == sf.repo_id)
            .and_then(|(_, repo, _)| repo.source.as_ref())
            .map(|s| serde_json::to_string(s).unwrap_or_default())
            .unwrap_or_default();
        for skill in &sf.items {
            // Derive id from path components
            let skill_id = format!("{}/{}/{}", sf.repo_id, sf.flock_slug, skill.slug);
            let categories = serde_json::to_string(&skill.categories).unwrap_or_default();
            let keywords = serde_json::to_string(&skill.keywords).unwrap_or_default();
            let data = serde_json::to_string(&skill).unwrap_or_default();

            tx.execute(
                "INSERT OR REPLACE INTO skills (id, flock_id, repo_id, slug, name, path, summary, description, version, status, license, categories_json, keywords_json, source_json, entry_json, data_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                params![
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
                ],
            )?;
            total_skills += 1;
        }
    }

    // Restore installed state
    for (slug, at) in &installed_map {
        tx.execute(
            "UPDATE skills SET installed = 1, installed_at = ?1 WHERE slug = ?2",
            params![at, slug],
        )?;
    }

    // Update sync state
    let now = chrono::Utc::now().to_rfc3339();
    tx.execute(
        "INSERT OR REPLACE INTO sync_state (key, value) VALUES ('commit_sha', ?1)",
        params![commit_sha],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO sync_state (key, value) VALUES ('synced_at', ?1)",
        params![now],
    )?;

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

/// List skills with optional search, pagination, and status filter.
pub fn list_skills(
    query: Option<&str>,
    status_filter: Option<&str>,
    page: usize,
    page_size: usize,
) -> Result<(Vec<RegistrySkill>, usize)> {
    let conn = open_cache()?;
    let offset = page * page_size;

    let (where_clause, search_params) = build_search_clause(query, status_filter);

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
        "SELECT data_json FROM skills WHERE sign = ?1 LIMIT 1",
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

/// List all flock slugs from the registry cache.
pub fn list_flock_slugs() -> Result<Vec<String>> {
    let conn = open_cache()?;
    let mut stmt = conn.prepare("SELECT slug FROM flocks ORDER BY slug ASC")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut slugs = Vec::new();
    for row in rows {
        slugs.push(row?);
    }
    Ok(slugs)
}

/// Get total skill count from cache.
pub fn skill_count() -> Result<usize> {
    let conn = open_cache()?;
    let count: usize = conn.query_row("SELECT COUNT(*) FROM skills", [], |row| row.get(0))?;
    Ok(count)
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

fn build_search_clause(query: Option<&str>, status: Option<&str>) -> (String, Vec<String>) {
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
