//! Registry access backed by the Savhub REST API.
//!
//!  Registry data is fetched directly from the configured server. The only local state kept
//! here is lightweight fetched-skill metadata in JSON under `~/.savhub/`.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use reqwest::blocking::{Client, Response};
use reqwest::{Method, Url};
use savhub_shared::{
    FlockDetailResponse, FlockSummary, ImportedSkillRecord, PagedResponse, RepoDetailResponse,
    SecurityStatus, SecuritySummary, SkillDetailResponse, SkillListItem,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::get_config_dir;
use crate::skills::{RepoSkillOrigin, copy_skill_folder, write_repo_skill_origin};

pub use savhub_shared::{
    DataSource, FetchedSkillEntry, RegistryFlock, RegistrySkill, RemoteSkillFetchSpec,
    SkillEntry,
};

const DEFAULT_API_BASE: &str = "https://savhub.ai/api/v1";
const PAGE_LIMIT: usize = 100;

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
    Some(dir.join("config.toml"))
}

pub fn read_api_base_url() -> Option<String> {
    if let Some(path) = user_config_path()
        && let Ok(raw) = fs::read_to_string(&path)
    {
        let parsed = toml::from_str::<UserConfigFile>(&raw).ok();
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

#[derive(Debug, Clone)]
pub struct FetchedSkillInfo {
    pub slug: String,
    pub repo_sign: String,
    pub skill_path: String,
    pub local_path: PathBuf,
}

fn fetched_skills_path() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("fetched_skills.json"))
}

fn legacy_installed_skills_path() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("installed_skills.json"))
}

fn ensure_fetched_skills_path() -> Result<PathBuf> {
    let path = fetched_skills_path()?;
    if path.exists() {
        return Ok(path);
    }

    let legacy_path = legacy_installed_skills_path()?;
    if !legacy_path.exists() {
        return Ok(path);
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    match fs::rename(&legacy_path, &path) {
        Ok(()) => Ok(path),
        Err(_) => {
            let raw = fs::read(&legacy_path)?;
            fs::write(&path, raw)?;
            let _ = fs::remove_file(&legacy_path);
            Ok(path)
        }
    }
}

fn repos_dir() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("repos"))
}

fn normalize_skill_repo_path(value: &str) -> String {
    value
        .trim()
        .replace('\\', "/")
        .trim_matches('/')
        .to_string()
}

pub fn read_fetched_skills_file() -> Result<Vec<FetchedSkillEntry>> {
    let path = ensure_fetched_skills_path()?;
    let Ok(raw) = fs::read_to_string(path) else {
        return Ok(Vec::new());
    };
    Ok(serde_json::from_str::<Vec<FetchedSkillEntry>>(&raw).unwrap_or_default())
}

fn write_fetched_skills_file(entries: &[FetchedSkillEntry]) -> Result<()> {
    let path = ensure_fetched_skills_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(entries)?),
    )?;
    Ok(())
}

fn upsert_fetched_entry(entries: &mut Vec<FetchedSkillEntry>, entry: FetchedSkillEntry) {
    if let Some(existing) = entries
        .iter_mut()
        .find(|current| current.slug == entry.slug)
    {
        existing.fetched_at = entry.fetched_at;
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

pub fn fetched_skill_local_path(entry: &FetchedSkillEntry) -> Option<PathBuf> {
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
        sign: String::new(),
        repo: item.repo_url,
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
        sign: String::new(),
        repo: detail.flock.repo_url,
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
    skill_version: Option<String>,
}

fn skill_descriptor_from_detail(detail: SkillDetailResponse) -> RemoteSkillDescriptor {
    let skill_path = detail.skill.path.clone();
    let repo_sign = detail.skill.repo_url.clone();
    let skill_version = normalize_non_empty(
        detail
            .latest_version
            .as_ref()
            .map(|value| value.version.clone())
            .or_else(|| detail.versions.first().map(|value| value.version.clone())),
    );
    RemoteSkillDescriptor {
        repo_sign,
        skill_path,
        skill_version,
    }
}

fn normalize_non_empty(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

pub fn fetch_version_label(skill_version: Option<&str>, git_rev: &str) -> String {
    if let Some(version) = skill_version
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return version.to_string();
    }
    let git_rev = git_rev.trim();
    if git_rev.is_empty() {
        "fetched".to_string()
    } else {
        git_rev.chars().take(12).collect()
    }
}

fn run_git(action: &str, cwd: Option<&Path>, args: Vec<String>) -> Result<String> {
    let mut command = Command::new("git");
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    command.args(args.iter().map(String::as_str));
    let output = command
        .output()
        .with_context(|| format!("failed to {action}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            Err(anyhow!("failed to {action}"))
        } else {
            Err(anyhow!("failed to {action}: {stderr}"))
        }
    }
}

fn current_git_head(repo_root: &Path) -> Option<String> {
    normalize_non_empty(
        run_git(
            "read current git HEAD",
            Some(repo_root),
            vec!["rev-parse".to_string(), "HEAD".to_string()],
        )
        .ok(),
    )
}

fn current_remote_url(repo_root: &Path) -> Option<String> {
    normalize_non_empty(
        run_git(
            "read current git remote URL",
            Some(repo_root),
            vec![
                "config".to_string(),
                "--get".to_string(),
                "remote.origin.url".to_string(),
            ],
        )
        .ok(),
    )
}

fn repo_checkout_dir(repo_sign: &str) -> Result<PathBuf> {
    Ok(repos_dir()?.join(Path::new(repo_sign)))
}

fn ensure_repo_checkout(repo_sign: &str, git_url: &str, git_rev: &str) -> Result<PathBuf> {
    let repo_root = repo_checkout_dir(repo_sign)?;
    let git_dir = repo_root.join(".git");

    if git_dir.is_dir() {
        let remote_url = current_remote_url(&repo_root);
        if remote_url.as_deref() != Some(git_url) {
            fs::remove_dir_all(&repo_root).with_context(|| {
                format!(
                    "failed to remove stale repo checkout at {}",
                    repo_root.display()
                )
            })?;
        }
    } else if repo_root.exists() {
        fs::remove_dir_all(&repo_root)
            .with_context(|| format!("failed to remove {}", repo_root.display()))?;
    }

    if !repo_root.join(".git").is_dir() {
        if let Some(parent) = repo_root.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        run_git(
            "clone remote repo",
            None,
            vec![
                "clone".to_string(),
                git_url.to_string(),
                repo_root.display().to_string(),
            ],
        )?;
    } else if current_git_head(&repo_root).as_deref() == Some(git_rev) {
        return Ok(repo_root);
    } else {
        run_git(
            "fetch remote repo",
            Some(&repo_root),
            vec![
                "fetch".to_string(),
                "--tags".to_string(),
                "--force".to_string(),
                "--prune".to_string(),
                "origin".to_string(),
            ],
        )?;
    }

    run_git(
        "checkout repo revision",
        Some(&repo_root),
        vec![
            "checkout".to_string(),
            "--force".to_string(),
            "--detach".to_string(),
            git_rev.to_string(),
        ],
    )?;

    let current_head = current_git_head(&repo_root).unwrap_or_default();
    if current_head != git_rev {
        return Err(anyhow!(
            "repo `{repo_sign}` checked out `{current_head}` instead of `{git_rev}`"
        ));
    }

    Ok(repo_root)
}

pub fn cache_remote_skill_from_repo(spec: &RemoteSkillFetchSpec) -> Result<PathBuf> {
    let repo_root = ensure_repo_checkout(&spec.repo_sign, &spec.git_url, &spec.git_rev)?;
    let skill_path = normalize_skill_repo_path(&spec.skill_path);
    if skill_path.is_empty() {
        return Err(anyhow!("skill path is empty for repo `{}`", spec.repo_sign));
    }
    let skill_root = repo_root.join(Path::new(&skill_path));
    let metadata = fs::metadata(&skill_root).with_context(|| {
        format!(
            "skill path `{}` was not found in repo `{}` at `{}`",
            spec.skill_path, spec.repo_sign, spec.git_rev
        )
    })?;
    if !metadata.is_dir() {
        return Err(anyhow!(
            "skill path `{}` is not a directory in repo `{}`",
            spec.skill_path,
            spec.repo_sign
        ));
    }
    Ok(skill_root)
}

pub fn install_remote_skill_from_repo(spec: &RemoteSkillFetchSpec, target: &Path) -> Result<()> {
    let skill_root = cache_remote_skill_from_repo(spec)?;
    copy_skill_folder(&skill_root, target)
        .with_context(|| format!("failed to install skill into {}", target.display()))
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
    let mut url = client.v1_url("/skills")?;
    url.query_pairs_mut()
        .append_pair("limit", "50")
        .append_pair("q", slug);
    let response = client.get_json_url::<PagedResponse<SkillListItem>>(url)?;
    let Some(skill) = response
        .items
        .into_iter()
        .find(|item| item.slug.eq_ignore_ascii_case(slug))
    else {
        return Ok(None);
    };
    Ok(client
        .get_json_opt::<SkillDetailResponse>(&format!("/skills/{}", skill.id))?
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
                || format!("{}/{}", item.repo_url, item.slug) == identifier
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
        .map(|flock| format!("{}/{}", flock.repo_url, flock.slug))
        .collect())
}

pub fn fetch_skills_batch(slugs: &[String]) -> Result<Vec<FetchedSkillInfo>> {
    let mut seen = BTreeSet::new();
    let mut requested = Vec::new();
    for slug in slugs {
        let slug = slug.trim();
        if !slug.is_empty() && seen.insert(slug.to_string()) {
            requested.push(slug.to_string());
        }
    }

    let mut fetched_entries = read_fetched_skills_file().unwrap_or_default();
    let mut results = Vec::new();

    for slug in requested {
        let descriptor = match remote_skill_detail(&slug)? {
            Some(value) => value,
            None => {
                eprintln!("  \x1b[33m!\x1b[0m {slug}: not found in registry API");
                continue;
            }
        };
        let repo = match remote_repo_detail(&descriptor.repo_sign)? {
            Some(value) => value,
            None => {
                eprintln!(
                    "  \x1b[33m!\x1b[0m {slug}: repo `{}` not found in registry API",
                    descriptor.repo_sign
                );
                continue;
            }
        };
        let Some(git_rev) = normalize_non_empty(repo.document.git_rev.clone()) else {
            eprintln!(
                "  \x1b[33m!\x1b[0m {slug}: repo `{}` has no git_rev",
                descriptor.repo_sign
            );
            continue;
        };
        let spec = RemoteSkillFetchSpec {
            repo_sign: descriptor.repo_sign.clone(),
            skill_path: descriptor.skill_path.clone(),
            git_url: repo.document.git_url.clone(),
            git_rev: git_rev.clone(),
            skill_version: descriptor.skill_version.clone(),
        };
        let local_path = match cache_remote_skill_from_repo(&spec) {
            Ok(path) => path,
            Err(error) => {
                eprintln!("  \x1b[33m!\x1b[0m {slug}: {error}");
                continue;
            }
        };

        let _ = write_repo_skill_origin(
            &local_path,
            &RepoSkillOrigin {
                version: 1,
                repo: registry_api_base(),
                repo_sign: descriptor.repo_sign.clone(),
                repo_commit: Some(git_rev),
                slug: slug.clone(),
                skill_version: descriptor.skill_version.clone(),
                fetched_at: Utc::now().timestamp_millis(),
            },
        );

        upsert_fetched_entry(
            &mut fetched_entries,
            FetchedSkillEntry {
                slug: slug.clone(),
                fetched_at: Utc::now().to_rfc3339(),
                repo: descriptor.repo_sign.clone(),
                path: normalize_skill_repo_path(&descriptor.skill_path),
                local_path: Some(local_path.display().to_string()),
            },
        );

        results.push(FetchedSkillInfo {
            slug,
            repo_sign: descriptor.repo_sign,
            skill_path: descriptor.skill_path,
            local_path,
        });
    }

    write_fetched_skills_file(&fetched_entries)?;
    Ok(results)
}
