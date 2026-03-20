//! Registry access backed by the Savhub REST API.
//!
//! Local SQLite registry cache and sync state have been removed. Registry data
//! is fetched directly from the configured server. The only local state kept
//! here is lightweight installed-skill metadata in JSON under `~/.savhub/`.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use reqwest::blocking::{Client, Response};
use reqwest::{Method, Url};
use savhub_shared::{
    FlockDetailResponse, FlockSummary, ImportedSkillRecord, PagedResponse, RepoDetailResponse,
    ResolveResponse, SecurityStatus, SkillDetailResponse, SkillListItem,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::get_config_dir;
use crate::skills::{RepoSkillOrigin, extract_zip_to_dir, write_repo_skill_origin};

const DEFAULT_API_BASE: &str = "https://savhub.ai/api/v1";
const PAGE_LIMIT: usize = 100;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecuritySummary {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reason_codes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scanned_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scanned_commit: Option<String>,
}

fn is_default_security(summary: &SecuritySummary) -> bool {
    *summary == SecuritySummary::default()
}

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

pub fn read_api_base_url() -> Option<String> {
    if let Some(path) = user_config_path()
        && let Ok(raw) = fs::read_to_string(&path)
    {
        let parsed = if crate::kdl_support::is_kdl_path(&path) {
            crate::kdl_support::parse_kdl::<UserConfigFile>(&raw).ok()
        } else {
            toml::from_str::<UserConfigFile>(&raw).ok()
        };
        if let Some(cfg) = parsed
            && let Some(url) = cfg
                .rest_api
                .and_then(|rest| rest.base_url)
                .filter(|value| !value.trim().is_empty())
        {
            return Some(url);
        }
    }
    None
}

fn registry_api_base() -> String {
    read_api_base_url()
        .or_else(|| {
            crate::config::read_global_config()
                .ok()
                .flatten()
                .and_then(|cfg| cfg.registry)
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_else(|| DEFAULT_API_BASE.to_string())
}

fn registry_api_token() -> Option<String> {
    crate::config::read_global_config()
        .ok()
        .flatten()
        .and_then(|cfg| cfg.token)
        .filter(|value| !value.trim().is_empty())
}

#[derive(Clone)]
struct RegistryApiClient {
    base: String,
    token: Option<String>,
    client: Client,
}

impl RegistryApiClient {
    fn new() -> Result<Self> {
        Ok(Self {
            base: registry_api_base(),
            token: registry_api_token(),
            client: Client::builder()
                .user_agent("savhub-local")
                .build()
                .context("failed to build registry API client")?,
        })
    }

    fn v1_url(&self, path: &str) -> Result<Url> {
        let path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
        let base = self.base.trim_end_matches('/');
        let full = if base.ends_with("/api/v1") {
            format!("{base}{path}")
        } else {
            format!("{base}/api/v1{path}")
        };
        Url::parse(&full).map_err(|error| anyhow!("invalid registry API URL: {error}"))
    }

    fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.get_json_url(self.v1_url(path)?)
    }

    fn get_json_opt<T: DeserializeOwned>(&self, path: &str) -> Result<Option<T>> {
        self.get_json_url_opt(self.v1_url(path)?)
    }

    fn get_json_url<T: DeserializeOwned>(&self, url: Url) -> Result<T> {
        let response = self.send(Method::GET, url)?;
        if response.status().is_success() {
            response
                .json::<T>()
                .context("invalid registry API response payload")
        } else {
            Err(parse_api_error(response))
        }
    }

    fn get_json_url_opt<T: DeserializeOwned>(&self, url: Url) -> Result<Option<T>> {
        let response = self.send(Method::GET, url)?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if response.status().is_success() {
            response
                .json::<T>()
                .map(Some)
                .context("invalid registry API response payload")
        } else {
            Err(parse_api_error(response))
        }
    }

    fn get_bytes(&self, path: &str) -> Result<Vec<u8>> {
        let response = self.send(Method::GET, self.v1_url(path)?)?;
        if response.status().is_success() {
            Ok(response
                .bytes()
                .context("failed to read registry API response body")?
                .to_vec())
        } else {
            Err(parse_api_error(response))
        }
    }

    fn send(&self, method: Method, url: Url) -> Result<Response> {
        let mut request = self.client.request(method, url);
        if let Some(token) = self.token.as_deref() {
            request = request.bearer_auth(token);
        }
        request.send().context("registry API request failed")
    }
}

fn parse_api_error(response: Response) -> anyhow::Error {
    let status = response.status();
    let body = response.text().unwrap_or_default();
    if let Ok(payload) = serde_json::from_str::<Value>(&body)
        && let Some(message) = payload.get("error").and_then(Value::as_str)
    {
        return anyhow!("{}: {}", status.as_u16(), message);
    }
    if body.trim().is_empty() {
        anyhow!("registry API error: {status}")
    } else {
        anyhow!("registry API error {}: {}", status.as_u16(), body.trim())
    }
}

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegistryFlock {
    #[serde(default)]
    pub schema_version: u32,
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
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(default)]
    pub license: String,
    #[serde(default, skip_serializing_if = "is_default_security")]
    pub security: SecuritySummary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegistrySkill {
    pub slug: String,
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
    #[serde(default, skip_serializing_if = "is_default_security")]
    pub security: SecuritySummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataSource {
    Local,
    Remote,
}

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
    pub stars: Option<u32>,
    pub starred_by_me: Option<bool>,
    pub downloads: Option<u64>,
    pub owner: Option<String>,
    #[serde(default, skip_serializing_if = "is_default_security")]
    pub security: SecuritySummary,
    #[serde(skip)]
    pub data_source: Option<DataSource>,
}

impl From<RegistrySkill> for SkillEntry {
    fn from(skill: RegistrySkill) -> Self {
        Self {
            slug: skill.slug,
            name: skill.name,
            description: skill.description,
            version: skill.version,
            status: skill.status,
            license: skill.license,
            categories: skill.categories,
            keywords: skill.keywords,
            source: None,
            stars: None,
            starred_by_me: None,
            downloads: None,
            owner: None,
            security: skill.security,
            data_source: Some(DataSource::Remote),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SyncInfo {
    pub commit_sha: String,
    pub synced_at: String,
    pub skill_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledSkillEntry {
    pub slug: String,
    pub installed_at: String,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InstalledSkillInfo {
    pub slug: String,
    pub repo_sign: String,
    pub skill_path: String,
    pub local_path: PathBuf,
}

fn installed_skills_path() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("installed_skills.json"))
}

fn install_cache_root() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("skills-cache"))
}

fn repos_dir() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("repos"))
}

fn skill_cache_dir(slug: &str) -> Result<PathBuf> {
    Ok(install_cache_root()?.join(slug.trim()))
}

fn normalize_skill_repo_path(value: &str) -> String {
    value
        .trim()
        .replace('\\', "/")
        .trim_matches('/')
        .to_string()
}

pub fn read_installed_skills_file() -> Result<Vec<InstalledSkillEntry>> {
    let path = installed_skills_path()?;
    let Ok(raw) = fs::read_to_string(path) else {
        return Ok(Vec::new());
    };
    Ok(serde_json::from_str::<Vec<InstalledSkillEntry>>(&raw).unwrap_or_default())
}

fn write_installed_skills_file(entries: &[InstalledSkillEntry]) -> Result<()> {
    let path = installed_skills_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(entries)?),
    )?;
    Ok(())
}

fn upsert_installed_entry(entries: &mut Vec<InstalledSkillEntry>, entry: InstalledSkillEntry) {
    if let Some(existing) = entries
        .iter_mut()
        .find(|current| current.slug == entry.slug)
    {
        existing.installed_at = entry.installed_at;
        if !entry.repo.is_empty() {
            existing.repo = entry.repo;
        }
        if !entry.path.is_empty() {
            existing.path = entry.path;
        }
        if entry.local_path.is_some() {
            existing.local_path = entry.local_path;
        }
    } else {
        entries.push(entry);
    }
}

pub fn installed_skill_local_path(entry: &InstalledSkillEntry) -> Option<PathBuf> {
    if let Some(local_path) = entry
        .local_path
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        return Some(PathBuf::from(local_path));
    }
    if entry.repo.is_empty() || entry.path.is_empty() {
        return None;
    }
    let root = repos_dir().ok()?;
    Some(root.join(&entry.repo).join(Path::new(&entry.path)))
}

pub fn make_skill_sign(repo_sign: &str, skill_path: &str) -> String {
    format!("{repo_sign}/{skill_path}")
}

pub fn skill_matches_skipped(sign: &str, skipped: &[String]) -> bool {
    let slug = sign.rsplit('/').next().unwrap_or(sign);
    skipped.iter().any(|entry| {
        entry == sign
            || entry == slug
            || entry.rsplit('/').next().map(|value| value == slug) == Some(true)
    })
}

fn enum_string<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value)
        .unwrap_or_else(|_| "\"unknown\"".to_string())
        .trim_matches('"')
        .to_string()
}

fn security_from_status(status: SecurityStatus) -> SecuritySummary {
    SecuritySummary {
        status: Some(enum_string(&status)),
        ..SecuritySummary::default()
    }
}

fn registry_skill_from_list_item(item: SkillListItem) -> RegistrySkill {
    RegistrySkill {
        slug: item.slug,
        path: item.path,
        name: item.display_name,
        description: item.summary,
        version: item.latest_version.map(|value| value.version),
        status: "active".to_string(),
        license: String::new(),
        categories: Vec::new(),
        keywords: Vec::new(),
        security: SecuritySummary::default(),
    }
}

fn registry_skill_from_imported(item: ImportedSkillRecord) -> RegistrySkill {
    RegistrySkill {
        slug: item.slug,
        path: item.path,
        name: item.name,
        description: item.description,
        version: item.version,
        status: enum_string(&item.status),
        license: item.license,
        categories: Vec::new(),
        keywords: Vec::new(),
        security: SecuritySummary::default(),
    }
}

fn registry_flock_from_summary(item: FlockSummary) -> RegistryFlock {
    RegistryFlock {
        schema_version: 1,
        sign: format!("{}/{}", item.repo_sign, item.slug),
        repo: item.repo_sign,
        slug: item.slug,
        name: item.name,
        description: item.description,
        path: None,
        version: item.version,
        status: enum_string(&item.status),
        visibility: item.visibility.map(|value| enum_string(&value)),
        license: item.license,
        security: security_from_status(item.security_status),
    }
}

fn registry_flock_from_detail(detail: FlockDetailResponse) -> RegistryFlock {
    RegistryFlock {
        schema_version: 1,
        sign: format!("{}/{}", detail.flock.repo_sign, detail.flock.slug),
        repo: detail.flock.repo_sign,
        slug: detail.flock.slug,
        name: detail.flock.name,
        description: detail.flock.description,
        path: detail.document.path,
        version: detail.flock.version,
        status: enum_string(&detail.flock.status),
        visibility: detail.flock.visibility.map(|value| enum_string(&value)),
        license: detail.flock.license,
        security: security_from_status(detail.flock.security_status),
    }
}

#[derive(Debug, Clone)]
struct RemoteSkillDescriptor {
    repo_sign: String,
    skill_path: String,
}

fn skill_descriptor_from_detail(detail: SkillDetailResponse) -> RemoteSkillDescriptor {
    let repo_sign = detail.skill.repo_id.clone();
    let skill_path = detail.skill.path.clone();
    RemoteSkillDescriptor {
        repo_sign,
        skill_path,
    }
}

fn fetch_skill_page(
    client: &RegistryApiClient,
    query: Option<&str>,
    page: usize,
    page_size: usize,
) -> Result<(Vec<SkillListItem>, bool)> {
    let mut url = client.v1_url("/skills")?;
    url.query_pairs_mut()
        .append_pair("limit", &page_size.to_string())
        .append_pair("sort", "updated")
        .append_pair("cursor", &(page.saturating_mul(page_size)).to_string());
    if let Some(query) = query.filter(|value| !value.trim().is_empty()) {
        url.query_pairs_mut().append_pair("q", query.trim());
    }
    let response = client.get_json_url::<PagedResponse<SkillListItem>>(url)?;
    Ok((response.items, response.next_cursor.is_some()))
}

fn fetch_flock_page(
    client: &RegistryApiClient,
    query: Option<&str>,
    page: usize,
    page_size: usize,
) -> Result<(Vec<FlockSummary>, bool)> {
    let mut url = client.v1_url("/flocks")?;
    url.query_pairs_mut()
        .append_pair("limit", &page_size.to_string())
        .append_pair("sort", "updated")
        .append_pair("cursor", &(page.saturating_mul(page_size)).to_string());
    if let Some(query) = query.filter(|value| !value.trim().is_empty()) {
        url.query_pairs_mut().append_pair("q", query.trim());
    }
    let response = client.get_json_url::<PagedResponse<FlockSummary>>(url)?;
    Ok((response.items, response.next_cursor.is_some()))
}

fn estimate_total(page: usize, page_size: usize, len: usize, has_more: bool) -> usize {
    let seen = page.saturating_mul(page_size).saturating_add(len);
    if has_more {
        seen.saturating_add(1)
    } else {
        seen
    }
}

fn remote_skill_detail(slug: &str) -> Result<Option<RemoteSkillDescriptor>> {
    let client = RegistryApiClient::new()?;
    Ok(client
        .get_json_opt::<SkillDetailResponse>(&format!("/skills/{slug}"))?
        .map(skill_descriptor_from_detail))
}

fn remote_repo_detail(repo_sign: &str) -> Result<Option<RepoDetailResponse>> {
    RegistryApiClient::new()?.get_json_opt(&format!("/repos/{repo_sign}"))
}

fn remote_flock_detail_by_id(id: &str) -> Result<Option<FlockDetailResponse>> {
    RegistryApiClient::new()?.get_json_opt(&format!("/flocks/{id}"))
}

fn find_flock_summary(identifier: &str) -> Result<Option<FlockSummary>> {
    let identifier = identifier.trim();
    if identifier.is_empty() {
        return Ok(None);
    }
    if let Some(detail) = remote_flock_detail_by_id(identifier)? {
        return Ok(Some(detail.flock));
    }
    if let Some((repo_sign, flock_slug)) = identifier.rsplit_once('/')
        && repo_sign.contains('.')
        && let Some(repo) = remote_repo_detail(repo_sign)?
        && let Some(flock) = repo.flocks.into_iter().find(|item| item.slug == flock_slug)
    {
        return Ok(Some(flock));
    }
    let client = RegistryApiClient::new()?;
    let mut page = 0usize;
    loop {
        let (items, has_more) = fetch_flock_page(&client, Some(identifier), page, PAGE_LIMIT)?;
        if let Some(flock) = items.into_iter().find(|item| {
            item.slug == identifier
                || item.id.to_string() == identifier
                || format!("{}/{}", item.repo_sign, item.slug) == identifier
        }) {
            return Ok(Some(flock));
        }
        if !has_more {
            return Ok(None);
        }
        page += 1;
    }
}

fn remote_flock_detail(identifier: &str) -> Result<Option<FlockDetailResponse>> {
    if let Some(detail) = remote_flock_detail_by_id(identifier)? {
        return Ok(Some(detail));
    }
    let Some(summary) = find_flock_summary(identifier)? else {
        return Ok(None);
    };
    remote_flock_detail_by_id(&summary.id.to_string())
}

fn resolve_latest_version(client: &RegistryApiClient, slug: &str) -> Result<String> {
    let resolved =
        client.get_json::<ResolveResponse>(&format!("/skills/{slug}/resolve?tag=latest"))?;
    resolved
        .matched
        .or(resolved.latest_version)
        .map(|value| value.version)
        .ok_or_else(|| anyhow!("skill '{slug}' has no downloadable version"))
}

pub fn ensure_registry_synced() -> Result<bool> {
    Ok(false)
}

pub fn cached_commit_sha() -> Result<Option<String>> {
    Ok(None)
}

pub fn sync_info() -> Result<Option<SyncInfo>> {
    Ok(None)
}

pub fn list_skills(
    query: Option<&str>,
    _status_filter: Option<&str>,
    page: usize,
    page_size: usize,
) -> Result<(Vec<RegistrySkill>, usize)> {
    let client = RegistryApiClient::new()?;
    let (items, has_more) = fetch_skill_page(&client, query, page, page_size)?;
    let mapped = items
        .into_iter()
        .map(registry_skill_from_list_item)
        .collect::<Vec<_>>();
    let total = estimate_total(page, page_size, mapped.len(), has_more);
    Ok((mapped, total))
}

pub fn search_skills(query: &str, limit: usize) -> Result<Vec<RegistrySkill>> {
    let (skills, _) = list_skills(Some(query), None, 0, limit)?;
    Ok(skills)
}

pub fn list_flocks() -> Result<Vec<RegistryFlock>> {
    let client = RegistryApiClient::new()?;
    let mut page = 0usize;
    let mut all = Vec::new();
    loop {
        let (items, has_more) = fetch_flock_page(&client, None, page, PAGE_LIMIT)?;
        if items.is_empty() {
            return Ok(all);
        }
        all.extend(items.into_iter().map(registry_flock_from_summary));
        if !has_more {
            return Ok(all);
        }
        page += 1;
    }
}

pub fn get_flock_by_slug(slug: &str) -> Result<Option<RegistryFlock>> {
    if let Some(detail) = remote_flock_detail(slug)? {
        return Ok(Some(registry_flock_from_detail(detail)));
    }
    Ok(find_flock_summary(slug)?.map(registry_flock_from_summary))
}

pub fn list_skills_in_flock(flock_slug: &str) -> Result<Vec<RegistrySkill>> {
    let Some(detail) = remote_flock_detail(flock_slug)? else {
        return Ok(Vec::new());
    };
    Ok(detail
        .skills
        .into_iter()
        .map(registry_skill_from_imported)
        .collect())
}

pub fn list_flock_skill_slugs(flock_slug: &str) -> Result<Vec<String>> {
    Ok(list_skills_in_flock(flock_slug)?
        .into_iter()
        .map(|skill| skill.slug)
        .collect())
}

pub fn list_repo_flock_signs(repo_sign: &str) -> Result<Vec<String>> {
    let Some(detail) = remote_repo_detail(repo_sign)? else {
        return Ok(Vec::new());
    };
    Ok(detail
        .flocks
        .into_iter()
        .map(|flock| format!("{}/{}", flock.repo_sign, flock.slug))
        .collect())
}

pub fn install_skills_batch(slugs: &[String]) -> Result<Vec<InstalledSkillInfo>> {
    let client = RegistryApiClient::new()?;
    fs::create_dir_all(install_cache_root()?)?;

    let mut seen = BTreeSet::new();
    let mut requested = Vec::new();
    for slug in slugs {
        let slug = slug.trim();
        if !slug.is_empty() && seen.insert(slug.to_string()) {
            requested.push(slug.to_string());
        }
    }

    let mut installed_entries = read_installed_skills_file().unwrap_or_default();
    let mut results = Vec::new();

    for slug in requested {
        let descriptor = match remote_skill_detail(&slug)? {
            Some(value) => value,
            None => {
                eprintln!("  \x1b[33m!\x1b[0m {slug}: not found in registry API");
                continue;
            }
        };
        let version = match resolve_latest_version(&client, &slug) {
            Ok(value) => value,
            Err(error) => {
                eprintln!("  \x1b[33m!\x1b[0m {slug}: {error}");
                continue;
            }
        };
        let bytes = match client.get_bytes(&format!("/skills/{slug}/versions/{version}/download")) {
            Ok(value) => value,
            Err(error) => {
                eprintln!("  \x1b[33m!\x1b[0m {slug}: {error}");
                continue;
            }
        };

        let local_path = skill_cache_dir(&slug)?;
        let _ = fs::remove_dir_all(&local_path);
        extract_zip_to_dir(&bytes, &local_path)
            .with_context(|| format!("failed to extract downloaded bundle for '{slug}'"))?;

        let _ = write_repo_skill_origin(
            &local_path,
            &RepoSkillOrigin {
                version: 1,
                repo: registry_api_base(),
                repo_sign: descriptor.repo_sign.clone(),
                repo_commit: None,
                slug: slug.clone(),
                skill_version: Some(version),
                installed_at: Utc::now().timestamp_millis(),
            },
        );

        upsert_installed_entry(
            &mut installed_entries,
            InstalledSkillEntry {
                slug: slug.clone(),
                installed_at: Utc::now().to_rfc3339(),
                repo: descriptor.repo_sign.clone(),
                path: normalize_skill_repo_path(&descriptor.skill_path),
                local_path: Some(local_path.display().to_string()),
            },
        );

        results.push(InstalledSkillInfo {
            slug,
            repo_sign: descriptor.repo_sign,
            skill_path: descriptor.skill_path,
            local_path,
        });
    }

    write_installed_skills_file(&installed_entries)?;
    Ok(results)
}

#[allow(dead_code)]
pub fn install_skill_from_registry(sign: &str) -> Result<PathBuf> {
    let slug = sign.rsplit('/').next().unwrap_or(sign).to_string();
    install_skills_batch(&[slug])?
        .into_iter()
        .next()
        .map(|item| item.local_path)
        .ok_or_else(|| anyhow!("skill '{sign}' could not be installed"))
}
