use std::collections::HashSet;
use std::path::Path;

use savhub_local::registry::{cache_remote_skill_from_repo, fetch_version_label};
use savhub_local::skills::FetchedSkillMetadata;
use savhub_shared::{
    DataSource, FlockDetailResponse, FlockSummary, PagedResponse, RemoteSkillFetchSpec,
    RepoDetailResponse, SkillDetailResponse, SkillEntry, SkillListItem,
};

// Re-export shared ApiClient and related types so existing desktop code keeps working.
pub use savhub_local::api::{ApiClient, ApiCompatibility, CLIENT_API_VERSION};

#[derive(Debug, Clone, Default)]
pub struct RemoteSkillLookup {
    pub local_slug: String,
    pub id: Option<String>,
    pub slug: Option<String>,
    pub repo_url: Option<String>,
    pub path: Option<String>,
    pub flock_slug: Option<String>,
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
    pub repo_url: String,
    pub path: String,
    pub version: String,
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
    let resp = client
        .get_json::<PagedResponse<SkillListItem>>(&format!(
            "/skills?limit={limit}{}{}",
            query
                .filter(|q| !q.is_empty())
                .map(|q| format!("&q={q}"))
                .unwrap_or_default(),
            cursor.map(|c| format!("&cursor={c}")).unwrap_or_default(),
        ))
        .await
        .map_err(|e| e.to_string())?;
    let entries = resp.items.iter().map(skill_list_item_to_entry).collect();
    Ok((entries, resp.next_cursor))
}

pub async fn fetch_remote_skill_page(
    client: &ApiClient,
    query: Option<&str>,
    limit: usize,
    page_index: usize,
) -> Result<(Vec<SkillListItem>, bool), String> {
    let mut url = client.v1_url("/skills").map_err(|e| e.to_string())?;
    url.query_pairs_mut()
        .append_pair("limit", &limit.to_string())
        .append_pair("sort", "updated")
        .append_pair("cursor", &(page_index.saturating_mul(limit)).to_string());
    if let Some(q) = query.filter(|q| !q.trim().is_empty()) {
        url.query_pairs_mut().append_pair("q", q.trim());
    }
    let resp = client
        .get_json_url::<PagedResponse<SkillListItem>>(url)
        .await
        .map_err(|e| e.to_string())?;
    Ok((resp.items, resp.next_cursor.is_some()))
}

pub async fn fetch_remote_flock_page(
    client: &ApiClient,
    query: Option<&str>,
    limit: usize,
    page_index: usize,
) -> Result<(Vec<FlockSummary>, bool), String> {
    let mut url = client.v1_url("/flocks").map_err(|e| e.to_string())?;
    url.query_pairs_mut()
        .append_pair("limit", &limit.to_string())
        .append_pair("sort", "updated")
        .append_pair("cursor", &(page_index.saturating_mul(limit)).to_string());
    if let Some(q) = query.filter(|q| !q.trim().is_empty()) {
        url.query_pairs_mut().append_pair("q", q.trim());
    }
    let resp = client
        .get_json_url::<PagedResponse<FlockSummary>>(url)
        .await
        .map_err(|e| e.to_string())?;
    Ok((resp.items, resp.next_cursor.is_some()))
}

pub async fn fetch_remote_flock_detail(
    client: &ApiClient,
    id: &str,
) -> Result<FlockDetailResponse, String> {
    client
        .get_json::<FlockDetailResponse>(&format!("/flocks/{id}"))
        .await
        .map_err(|e| e.to_string())
}

pub async fn fetch_remote_skill_detail(
    client: &ApiClient,
    id: &str,
) -> Result<SkillDetailResponse, String> {
    client
        .get_json::<SkillDetailResponse>(&format!("/skills/{id}"))
        .await
        .map_err(|e| e.to_string())
}

pub async fn fetch_remote_repo_detail(
    client: &ApiClient,
    repo_sign: &str,
) -> Result<RepoDetailResponse, String> {
    client
        .get_json::<RepoDetailResponse>(&format!("/repos/{repo_sign}"))
        .await
        .map_err(|e| e.to_string())
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
    let repo_url = detail.skill.repo_url.trim();
    if repo_url.is_empty() {
        return Err(format!(
            "skill `{}` is missing repo_url metadata",
            detail.skill.slug
        ));
    }
    Ok(repo_url.to_string())
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

    // If we have repo_url + path, search by slug and filter by repo_url match
    if let Some(repo_url) = lookup
        .repo_url
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        if let Some(path) = lookup.path.as_deref().filter(|v| !v.trim().is_empty()) {
            let q = path.rsplit('/').next().unwrap_or(path);
            let mut url = client.v1_url("/skills").map_err(|e| e.to_string())?;
            url.query_pairs_mut()
                .append_pair("limit", "20")
                .append_pair("q", q);
            let response = client
                .get_json_url::<PagedResponse<SkillListItem>>(url)
                .await
                .map_err(|e| e.to_string())?;
            if let Some(item) = response
                .items
                .into_iter()
                .find(|item| item.repo_url == repo_url && item.path == path)
            {
                return Ok(item);
            }
        }
    }

    let queries = collect_skill_queries(&lookup);
    let mut seen = HashSet::new();
    let mut candidates = Vec::new();

    for query in queries {
        let mut url = client.v1_url("/skills").map_err(|e| e.to_string())?;
        url.query_pairs_mut()
            .append_pair("limit", "50")
            .append_pair("q", &query);
        let response = client
            .get_json_url::<PagedResponse<SkillListItem>>(url)
            .await
            .map_err(|e| e.to_string())?;
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
    let flock_slug = lookup.flock_slug.clone();
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

    // Clone/update the repo in ~/.savhub/repos/ — no copy to a separate skills dir.
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        cache_remote_skill_from_repo(&spec)
            .map(|_| ())
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("failed to join fetch task: {error}"))??;

    // Track in lockfile only.
    savhub_local::skills::update_lockfile_with_metadata(
        workdir,
        &install_slug,
        &version,
        &FetchedSkillMetadata {
            remote_id: Some(skill.id.to_string()),
            remote_slug: Some(skill.slug.clone()),
            repo_url: Some(skill.repo_url.clone()),
            path: Some(skill.path.clone()),
            flock_slug: flock_slug.clone(),
            git_rev: Some(git_rev.clone()),
        },
    );

    Ok(FetchedRemoteSkill {
        local_slug: install_slug,
        remote_id: skill.id.to_string(),
        remote_slug: skill.slug,
        repo_url: skill.repo_url,
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
        .map(|item| format!("{}/{}", item.repo_url, item.slug))
        .collect())
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
    // repo_url is not useful as a search query, skip it
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
    let item_path = item.path.to_ascii_lowercase();
    let item_path_base = path_basename(&item.path).map(|value| value.to_ascii_lowercase());
    let item_name = item.display_name.to_ascii_lowercase();
    let item_repo_url = item.repo_url.to_ascii_lowercase();

    let local_slug = lookup.local_slug.trim().to_ascii_lowercase();
    let lookup_slug = lookup
        .slug
        .as_deref()
        .map(|value| value.trim().to_ascii_lowercase());
    let lookup_repo_url = lookup
        .repo_url
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

    let mut score = 0;

    // Exact repo_url + path match is the strongest signal
    if lookup_repo_url.as_deref() == Some(item_repo_url.as_str())
        && lookup_path.as_deref() == Some(item_path.as_str())
    {
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
    if lookup_path_base.as_deref() == item_path_base.as_deref() && lookup_path_base.is_some() {
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
