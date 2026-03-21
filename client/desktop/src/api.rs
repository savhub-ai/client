use std::collections::HashSet;
use std::path::Path;

use reqwest::{Client, Method, Response, Url};
use savhub_local::registry::{fetch_version_label, install_remote_skill_from_repo};
use savhub_local::skills::{RepoSkillOrigin, write_repo_skill_origin};
use savhub_shared::{
    DataSource, FlockDetailResponse, FlockSummary, PagedResponse, RemoteSkillFetchSpec,
    RepoDetailResponse, SkillDetailResponse, SkillEntry, SkillListItem,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// The API version this client accepts. Registry must return the same major
/// version in `/health` → `apiVersion`, otherwise the user is warned.
pub const CLIENT_API_VERSION: u32 = 1;

/// Result of comparing client and registry API versions.
#[derive(Debug, Clone, PartialEq)]
pub enum ApiCompatibility {
    /// Not yet checked.
    Unknown,
    /// Versions are compatible (same major).
    Compatible { registry_version: u32 },
    /// Major version mismatch – client must be updated.
    Incompatible { registry_version: u32 },
}

#[derive(Clone)]
pub struct ApiClient {
    pub base: String,
    client: Client,
    pub token: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RemoteSkillLookup {
    pub local_slug: String,
    pub id: Option<String>,
    pub slug: Option<String>,
    pub sign: Option<String>,
    pub path: Option<String>,
}

impl RemoteSkillLookup {
    pub fn from_local_slug(local_slug: impl Into<String>) -> Self {
        let local_slug = local_slug.into();
        Self {
            local_slug: local_slug.clone(),
            slug: Some(local_slug),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone)]
pub struct FetchedRemoteSkill {
    pub local_slug: String,
    pub remote_id: String,
    pub remote_slug: String,
    pub sign: String,
    pub path: String,
    pub version: String,
}

impl ApiClient {
    pub fn new(base: impl Into<String>, token: Option<String>) -> Self {
        Self {
            base: base.into(),
            client: Client::new(),
            token,
        }
    }

    pub fn v1_url(&self, path: &str) -> Result<Url, String> {
        let path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
        let base = self.base.trim_end_matches('/');
        // If base already ends with /api/v1, don't append it again
        let full = if base.ends_with("/api/v1") || base.ends_with("/api/v1/") {
            format!("{base}{path}")
        } else {
            format!("{base}/api/v1{path}")
        };
        Url::parse(&full).map_err(|e| format!("invalid API base URL: {e}"))
    }

    pub async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        let url = self.v1_url(path)?;
        self.request_json::<(), T>(Method::GET, url, None).await
    }

    pub async fn get_json_url<T: DeserializeOwned>(&self, url: Url) -> Result<T, String> {
        self.request_json::<(), T>(Method::GET, url, None).await
    }

    pub async fn post_json<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, String> {
        let url = self.v1_url(path)?;
        self.request_json(Method::POST, url, Some(body)).await
    }

    pub async fn post_empty<T: DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        let url = self.v1_url(path)?;
        self.request_json::<(), T>(Method::POST, url, None).await
    }

    #[allow(dead_code)]
    pub async fn delete_json<T: DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        let url = self.v1_url(path)?;
        self.request_json::<(), T>(Method::DELETE, url, None).await
    }

    async fn request_json<B: Serialize, T: DeserializeOwned>(
        &self,
        method: Method,
        url: Url,
        body: Option<&B>,
    ) -> Result<T, String> {
        let request_method = method.clone();
        let request_url = url.clone();
        let request_body = body.and_then(|value| serde_json::to_string(value).ok());
        let response = self.send(method, url, body).await?;
        if response.status().is_success() {
            let status = response.status();
            let final_url = response.url().clone();
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("<unknown>")
                .to_string();
            let text = response.text().await.map_err(|e| {
                println!(
                    "error reading response body\nmethod: {request_method}\nurl: {request_url}\nfinal_url: {final_url}\nstatus: {status}\ncontent_type: {content_type}\nrequest_body: {}\nerror: {e}",
                    request_body.as_deref().unwrap_or("<none>")
                );
                format!("error reading response body: {e}")
            })?;
            serde_json::from_str::<T>(&text).map_err(|e| {
                println!(
                    "error decoding response body\nmethod: {request_method}\nurl: {request_url}\nfinal_url: {final_url}\nstatus: {status}\ncontent_type: {content_type}\nexpected_type: {}\nrequest_body: {}\nresponse_body: {}",
                    std::any::type_name::<T>(),
                    request_body.as_deref().unwrap_or("<none>"),
                    response_preview(&text)
                );
                format!("error decoding response body: {e}")
            })
        } else {
            Err(parse_error(response).await)
        }
    }

    async fn send<B: Serialize>(
        &self,
        method: Method,
        url: Url,
        body: Option<&B>,
    ) -> Result<Response, String> {
        let mut request = self.client.request(method, url);
        if let Some(token) = self.token.as_deref().filter(|t| !t.is_empty()) {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request.json(body);
        }
        request
            .send()
            .await
            .map_err(|e| format!("request failed: {e}"))
    }
}

impl ApiClient {
    /// Check the registry `/health` endpoint and compare `apiVersion` with
    /// [`CLIENT_API_VERSION`]. Returns the compatibility status.
    pub async fn check_api_compatibility(&self) -> ApiCompatibility {
        let Ok(value) = self.get_json::<Value>("/health").await else {
            return ApiCompatibility::Unknown;
        };
        let registry_version = value
            .get("apiVersion")
            .and_then(Value::as_u64)
            .unwrap_or(CLIENT_API_VERSION as u64) as u32;

        if registry_version == CLIENT_API_VERSION {
            ApiCompatibility::Compatible { registry_version }
        } else {
            ApiCompatibility::Incompatible { registry_version }
        }
    }
}

impl ApiClient {
    /// Download the registry zip archive from GitHub and return (bytes, commit_sha).
    #[allow(dead_code)]
    pub async fn download_registry_zip(
        &self,
        github_repo: &str,
    ) -> Result<(Vec<u8>, String), String> {
        // Get latest commit SHA
        let api_url = format!("https://api.github.com/repos/{github_repo}/commits/main");
        let sha_response = self
            .client
            .get(&api_url)
            .header("User-Agent", "savhub-desktop")
            .send()
            .await
            .map_err(|e| format!("failed to check registry: {e}"))?;
        let sha_json: serde_json::Value = sha_response
            .json()
            .await
            .map_err(|e| format!("invalid response: {e}"))?;
        let commit_sha = sha_json
            .get("sha")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();

        // Download zipball
        let zip_url = format!("https://api.github.com/repos/{github_repo}/zipball/main");
        let zip_response = self
            .client
            .get(&zip_url)
            .header("User-Agent", "savhub-desktop")
            .send()
            .await
            .map_err(|e| format!("failed to download registry: {e}"))?;

        if !zip_response.status().is_success() {
            return Err(format!(
                "registry download failed: {}",
                zip_response.status()
            ));
        }

        let bytes = zip_response
            .bytes()
            .await
            .map_err(|e| format!("failed to read registry data: {e}"))?
            .to_vec();

        Ok((bytes, commit_sha))
    }
}

/// Convert a server API `SkillListItem` into our unified `SkillEntry`.
#[allow(dead_code)]
pub fn skill_list_item_to_entry(item: &SkillListItem) -> SkillEntry {
    let version = item.latest_version.as_ref().map(|v| v.version.clone());
    SkillEntry {
        slug: item.slug.clone(),
        name: item.display_name.clone(),
        description: item.summary.clone(),
        version,
        status: "active".to_string(),
        license: String::new(),
        categories: Vec::new(),
        keywords: Vec::new(),
        source: None,
        stars: Some(item.stats.stars as u32),
        starred_by_me: None,
        downloads: Some(item.stats.downloads as u64),
        owner: Some(item.owner.handle.clone()),
        security: Default::default(),
        data_source: Some(DataSource::Remote),
    }
}

/// Fetch skills from the remote API as `SkillEntry` items.
/// Returns `(entries, next_cursor)` or an error.
#[allow(dead_code)]
pub async fn fetch_remote_skills(
    client: &ApiClient,
    query: Option<&str>,
    limit: usize,
    cursor: Option<&str>,
) -> Result<(Vec<SkillEntry>, Option<String>), String> {
    let mut url = client.v1_url("/skills")?;
    url.query_pairs_mut()
        .append_pair("limit", &limit.to_string());
    if let Some(q) = query.filter(|q| !q.is_empty()) {
        url.query_pairs_mut().append_pair("q", q);
    }
    if let Some(c) = cursor {
        url.query_pairs_mut().append_pair("cursor", c);
    }
    let resp = client
        .get_json::<PagedResponse<SkillListItem>>(&format!(
            "/skills?limit={limit}{}{}",
            query
                .filter(|q| !q.is_empty())
                .map(|q| format!("&q={q}"))
                .unwrap_or_default(),
            cursor.map(|c| format!("&cursor={c}")).unwrap_or_default(),
        ))
        .await?;
    let entries = resp.items.iter().map(skill_list_item_to_entry).collect();
    Ok((entries, resp.next_cursor))
}

pub async fn fetch_remote_skill_page(
    client: &ApiClient,
    query: Option<&str>,
    limit: usize,
    page_index: usize,
) -> Result<(Vec<SkillListItem>, bool), String> {
    let mut url = client.v1_url("/skills")?;
    url.query_pairs_mut()
        .append_pair("limit", &limit.to_string())
        .append_pair("sort", "updated")
        .append_pair("cursor", &(page_index.saturating_mul(limit)).to_string());
    if let Some(q) = query.filter(|q| !q.trim().is_empty()) {
        url.query_pairs_mut().append_pair("q", q.trim());
    }
    let resp = client
        .get_json_url::<PagedResponse<SkillListItem>>(url)
        .await?;
    Ok((resp.items, resp.next_cursor.is_some()))
}

pub async fn fetch_remote_flock_page(
    client: &ApiClient,
    query: Option<&str>,
    limit: usize,
    page_index: usize,
) -> Result<(Vec<FlockSummary>, bool), String> {
    let mut url = client.v1_url("/flocks")?;
    url.query_pairs_mut()
        .append_pair("limit", &limit.to_string())
        .append_pair("sort", "updated")
        .append_pair("cursor", &(page_index.saturating_mul(limit)).to_string());
    if let Some(q) = query.filter(|q| !q.trim().is_empty()) {
        url.query_pairs_mut().append_pair("q", q.trim());
    }
    let resp = client
        .get_json_url::<PagedResponse<FlockSummary>>(url)
        .await?;
    Ok((resp.items, resp.next_cursor.is_some()))
}

pub async fn fetch_remote_flock_detail(
    client: &ApiClient,
    id: &str,
) -> Result<FlockDetailResponse, String> {
    client
        .get_json::<FlockDetailResponse>(&format!("/flocks/{id}"))
        .await
}

pub async fn fetch_remote_skill_detail(
    client: &ApiClient,
    id: &str,
) -> Result<SkillDetailResponse, String> {
    client
        .get_json::<SkillDetailResponse>(&format!("/skills/{id}"))
        .await
}

pub async fn fetch_remote_repo_detail(
    client: &ApiClient,
    repo_sign: &str,
) -> Result<RepoDetailResponse, String> {
    client
        .get_json::<RepoDetailResponse>(&format!("/repos/{repo_sign}"))
        .await
}

fn normalize_remote_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn repo_sign_from_skill_detail(detail: &SkillDetailResponse) -> Result<String, String> {
    let sign = detail.skill.sign.trim();
    let skill_path = detail.skill.path.trim().trim_matches('/');
    if sign.is_empty() {
        return Err(format!(
            "skill `{}` is missing sign metadata",
            detail.skill.slug
        ));
    }
    if skill_path.is_empty() {
        return Ok(sign.to_string());
    }
    let suffix = format!("/{skill_path}");
    sign.strip_suffix(&suffix)
        .map(|value| value.to_string())
        .ok_or_else(|| {
            format!(
                "failed to derive repo sign from skill sign `{}` and path `{}`",
                detail.skill.sign, detail.skill.path
            )
        })
}

pub async fn resolve_remote_skill(
    client: &ApiClient,
    lookup: RemoteSkillLookup,
) -> Result<SkillListItem, String> {
    if let Some(id) = lookup
        .id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        match fetch_remote_skill_detail(client, id).await {
            Ok(detail) => return Ok(detail.skill),
            Err(err) if is_missing_skill_lookup_error(&err) => {}
            Err(err) => return Err(err),
        }
    }

    if let Some(sign) = lookup
        .sign
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        let mut url = client.v1_url("/skills")?;
        url.query_pairs_mut()
            .append_pair("limit", "20")
            .append_pair("sign", sign);
        let response = client
            .get_json_url::<PagedResponse<SkillListItem>>(url)
            .await?;
        if let Some(item) = response
            .items
            .into_iter()
            .find(|item| item.sign.eq_ignore_ascii_case(sign))
        {
            return Ok(item);
        }
    }

    let queries = collect_skill_queries(&lookup);
    let mut seen = HashSet::new();
    let mut candidates = Vec::new();

    for query in queries {
        let mut url = client.v1_url("/skills")?;
        url.query_pairs_mut()
            .append_pair("limit", "50")
            .append_pair("q", &query);
        let response = client
            .get_json_url::<PagedResponse<SkillListItem>>(url)
            .await?;
        for item in response.items {
            if seen.insert(item.id.to_string()) {
                candidates.push(item);
            }
        }
    }

    select_best_skill(candidates, &lookup).ok_or_else(|| {
        let label = lookup.local_slug.trim().to_string();
        if label.is_empty() {
            "remote skill not found".to_string()
        } else {
            format!("remote skill not found for `{label}`")
        }
    })
}

pub async fn fetch_remote_skill_with_lookup(
    client: &ApiClient,
    workdir: &Path,
    lookup: RemoteSkillLookup,
) -> Result<FetchedRemoteSkill, String> {
    let local_slug = lookup.local_slug.trim().to_string();
    let skill = resolve_remote_skill(client, lookup).await?;
    let detail = fetch_remote_skill_detail(client, &skill.id.to_string()).await?;
    let repo_sign = repo_sign_from_skill_detail(&detail)?;
    let repo = fetch_remote_repo_detail(client, &repo_sign).await?;
    let git_rev = normalize_remote_text(repo.document.git_rev.clone())
        .ok_or_else(|| format!("repo `{repo_sign}` has no git_rev"))?;
    let skill_version = normalize_remote_text(
        detail
            .latest_version
            .as_ref()
            .map(|value| value.version.clone())
            .or_else(|| detail.versions.first().map(|value| value.version.clone())),
    );
    let version = fetch_version_label(skill_version.as_deref(), &git_rev);
    let spec = RemoteSkillFetchSpec {
        repo_sign: repo_sign.clone(),
        skill_path: detail.skill.path.clone(),
        git_url: repo.document.git_url,
        git_rev: git_rev.clone(),
        skill_version: skill_version.clone(),
    };
    let install_slug = if local_slug.is_empty() {
        skill.slug.clone()
    } else {
        local_slug.clone()
    };
    let skill_dir = workdir.join(&install_slug);
    let registry = client.base.clone();
    let spec_for_install = spec.clone();
    let skill_dir_for_install = skill_dir.clone();
    let install_slug_for_origin = install_slug.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        install_remote_skill_from_repo(&spec_for_install, &skill_dir_for_install)
            .map_err(|error| error.to_string())?;
        let fetched_at = chrono::Utc::now().timestamp_millis();
        write_repo_skill_origin(
            &skill_dir_for_install,
            &RepoSkillOrigin {
                version: 1,
                repo: registry,
                repo_sign: spec_for_install.repo_sign.clone(),
                repo_commit: Some(spec_for_install.git_rev.clone()),
                slug: install_slug_for_origin,
                skill_version: spec_for_install.skill_version.clone(),
                fetched_at,
            },
        )
        .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("failed to join fetch task: {error}"))??;
    crate::skills::update_lockfile_with_metadata(
        workdir,
        &install_slug,
        &version,
        &crate::skills::FetchedSkillMetadata {
            remote_id: Some(skill.id.to_string()),
            remote_slug: Some(skill.slug.clone()),
            sign: Some(skill.sign.clone()),
            path: Some(skill.path.clone()),
        },
    );

    Ok(FetchedRemoteSkill {
        local_slug: install_slug,
        remote_id: skill.id.to_string(),
        remote_slug: skill.slug,
        sign: skill.sign,
        path: skill.path,
        version,
    })
}

pub async fn fetch_remote_flock_slug_suggestions(
    client: &ApiClient,
    query: &str,
    limit: usize,
) -> Result<Vec<String>, String> {
    let (items, _) = fetch_remote_flock_page(client, Some(query), limit, 0).await?;
    Ok(items
        .into_iter()
        .map(|item| format!("{}/{}", item.repo_sign, item.slug))
        .collect())
}

async fn parse_error(response: Response) -> String {
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if let Ok(payload) = serde_json::from_str::<Value>(&text) {
        if let Some(message) = payload.get("error").and_then(Value::as_str) {
            return format!("{}: {}", status.as_u16(), message);
        }
    }
    if text.trim().is_empty() {
        status.to_string()
    } else {
        format!("{}: {}", status.as_u16(), text)
    }
}

fn response_preview(text: &str) -> String {
    const LIMIT: usize = 4096;
    let truncated: String = text.chars().take(LIMIT).collect();
    if text.chars().count() > LIMIT {
        format!("{truncated}...(truncated)")
    } else {
        truncated
    }
}

fn is_missing_skill_lookup_error(error: &str) -> bool {
    error.starts_with("400:") || error.starts_with("404:")
}

fn collect_skill_queries(lookup: &RemoteSkillLookup) -> Vec<String> {
    let mut queries = Vec::new();
    push_unique_nonempty(&mut queries, Some(lookup.local_slug.as_str()));
    push_unique_nonempty(&mut queries, lookup.slug.as_deref());
    push_unique_nonempty(&mut queries, lookup.path.as_deref());
    push_unique_nonempty(&mut queries, lookup.path.as_deref().and_then(path_basename));
    push_unique_nonempty(&mut queries, lookup.sign.as_deref().and_then(sign_tail));
    queries
}

fn push_unique_nonempty(values: &mut Vec<String>, candidate: Option<&str>) {
    let Some(candidate) = candidate.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };
    if values
        .iter()
        .any(|value| value.eq_ignore_ascii_case(candidate))
    {
        return;
    }
    values.push(candidate.to_string());
}

fn path_basename(value: &str) -> Option<&str> {
    value.rsplit('/').find(|segment| !segment.trim().is_empty())
}

fn sign_tail(value: &str) -> Option<&str> {
    value.rsplit('/').find(|segment| !segment.trim().is_empty())
}

fn select_best_skill(
    candidates: Vec<SkillListItem>,
    lookup: &RemoteSkillLookup,
) -> Option<SkillListItem> {
    if candidates.len() == 1 {
        return candidates.into_iter().next();
    }

    let mut best_item = None;
    let mut best_score = i32::MIN;

    for item in candidates {
        let score = score_skill_match(&item, lookup);
        if score > best_score {
            best_score = score;
            best_item = Some(item);
        }
    }

    if best_score > 0 { best_item } else { None }
}

fn score_skill_match(item: &SkillListItem, lookup: &RemoteSkillLookup) -> i32 {
    let item_slug = item.slug.to_ascii_lowercase();
    let item_sign = item.sign.to_ascii_lowercase();
    let item_path = item.path.to_ascii_lowercase();
    let item_path_base = path_basename(&item.path).map(|value| value.to_ascii_lowercase());
    let item_name = item.display_name.to_ascii_lowercase();

    let local_slug = lookup.local_slug.trim().to_ascii_lowercase();
    let lookup_slug = lookup
        .slug
        .as_deref()
        .map(|value| value.trim().to_ascii_lowercase());
    let lookup_sign = lookup
        .sign
        .as_deref()
        .map(|value| value.trim().to_ascii_lowercase());
    let lookup_path = lookup
        .path
        .as_deref()
        .map(|value| value.trim().to_ascii_lowercase());
    let lookup_path_base = lookup
        .path
        .as_deref()
        .and_then(path_basename)
        .map(str::to_ascii_lowercase);
    let lookup_sign_tail = lookup
        .sign
        .as_deref()
        .and_then(sign_tail)
        .map(str::to_ascii_lowercase);

    let mut score = 0;

    if lookup_sign.as_deref() == Some(item_sign.as_str()) {
        score += 1000;
    }
    if lookup_path.as_deref() == Some(item_path.as_str()) {
        score += 800;
    }
    if lookup_slug.as_deref() == Some(item_slug.as_str()) {
        score += 700;
    }
    if lookup_path_base.as_deref() == item_path_base.as_deref() {
        score += 500;
    }
    if lookup_sign_tail.as_deref() == item_path_base.as_deref() {
        score += 420;
    }
    if !local_slug.is_empty() {
        if local_slug == item_slug {
            score += 650;
        }
        if item_path_base.as_deref() == Some(local_slug.as_str()) {
            score += 560;
        }
        if item_slug.ends_with(&format!("-{local_slug}")) {
            score += 360;
        }
        if item_path == local_slug {
            score += 320;
        }
        if item_slug.contains(&local_slug) {
            score += 140;
        }
        if item_path.contains(&local_slug) {
            score += 120;
        }
        if item_name.contains(&local_slug) {
            score += 40;
        }
    }

    score
}
