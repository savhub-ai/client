use std::collections::HashMap;
use std::io::Write;

use chrono::Utc;
use diesel::prelude::*;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zip::write::SimpleFileOptions;

use crate::auth::{RequestUser, parse_role};
use crate::error::AppError;
use crate::markdown::render_markdown;
use crate::models::{AuditLogRow, FlockRow, NewAuditLogRow, SkillRow, SkillVersionRow, UserRow};
use crate::schema::{audit_logs, flocks, repos, skill_versions, skills, users};
use crate::state::app_state;
use shared::{
    AuditLogEntry, CatalogStats, ModerationStatus, ResourceFileSummary, SkillBadges, SkillListItem,
    StoredBundleFile, UserRole, UserSummary, VersionDetail, VersionScanSummary, VersionSummary,
    bundle_metadata_from_json,
};

/// Normalize a git URL to a canonical HTTPS form.
///
/// - `git@github.com:org/repo` → `https://github.com/org/repo.git`
/// - `https://github.com/org/repo` → `https://github.com/org/repo.git`
/// - `http://github.com/org/repo.git/` → `https://github.com/org/repo.git`
///
/// Ensures the same repo always produces the same URL regardless of
/// how the user typed it.
pub fn normalize_git_url(raw: &str) -> String {
    let url = raw.trim();
    // Strip URL fragment (#...) and query string (?...)
    let url = url.split('#').next().unwrap_or(url);
    let url = url.split('?').next().unwrap_or(url);
    let url = url.trim_end_matches('/');

    // git@host:path → https://host/path
    let url = if let Some(rest) = url.strip_prefix("git@") {
        if let Some((host, path)) = rest.split_once(':') {
            format!("https://{}/{}", host, path.trim_start_matches('/'))
        } else {
            url.to_string()
        }
    } else if url.starts_with("http://") {
        // Upgrade http → https
        format!("https://{}", &url["http://".len()..])
    } else if !url.starts_with("https://") {
        // Bare host/path — assume https
        format!("https://{url}")
    } else {
        url.to_string()
    };

    // Strip trailing slash again after transform
    let url = url.trim_end_matches('/').to_string();

    // Ensure .git suffix
    if url.ends_with(".git") {
        url
    } else {
        format!("{url}.git")
    }
}

/// Extract `(domain, path_slug)` from a **normalized** git URL.
///
/// The domain is the host (with port `:` replaced by `-`).
/// The path_slug is the URL path without `.git`, with `/` replaced by `-`.
///
/// Example: `https://github.com/anthropics/skills.git`
///   → `("github.com", "anthropics-skills")`
pub fn parse_git_url_parts(git_url: &str) -> (String, String) {
    let url = git_url
        .trim()
        .trim_end_matches('/')
        .trim_end_matches(".git");

    if let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    {
        if let Some((host, path)) = rest.split_once('/') {
            let domain = host.replace(':', "-");
            return (domain, path.to_string());
        }
        return (rest.replace(':', "-"), String::new());
    }

    // git@host:path (shouldn't happen after normalize, but handle anyway)
    if let Some(rest) = url.strip_prefix("git@") {
        if let Some((host, path)) = rest.split_once(':') {
            let domain = host.replace(':', "-");
            return (domain, path.to_string());
        }
    }

    ("unknown".to_string(), url.to_string())
}

#[derive(Debug, Clone)]
pub struct DownloadResult {
    pub filename: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

/// Derive the old-style repo "sign" from its normalized git_url.
///
/// E.g. `https://github.com/org/repo.git` → `github.com/org/repo`
pub fn derive_repo_sign(git_url: &str) -> String {
    let (domain, path) = parse_git_url_parts(git_url);
    format!("{domain}/{path}")
}

/// Reconstruct a normalized git_url from domain and path_slug.
///
/// E.g. `("github.com", "org/repo")` → `https://github.com/org/repo.git`
pub fn sign_to_git_url(domain: &str, path_slug: &str) -> String {
    format!("https://{domain}/{path_slug}.git")
}

pub fn db_conn() -> Result<crate::db::PgPooledConnection, AppError> {
    app_state()
        .pool
        .get()
        .map_err(|error| AppError::Internal(error.to_string()))
}

pub fn fetch_skill_by_slug(
    conn: &mut PgConnection,
    slug_value: &str,
) -> Result<Option<SkillRow>, AppError> {
    skills::table
        .filter(skills::slug.eq(slug_value))
        .select(SkillRow::as_select())
        .first::<SkillRow>(conn)
        .optional()
        .map_err(Into::into)
}

pub fn fetch_flock_by_slugs(
    conn: &mut PgConnection,
    repo_url: &str,
    flock_slug: &str,
) -> Result<FlockRow, AppError> {
    let repo_id = repos::table
        .filter(repos::git_url.eq(&repo_url))
        .select(repos::id)
        .first::<Uuid>(conn)
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("repo `{repo_url}` does not exist")))?;
    flocks::table
        .filter(flocks::repo_id.eq(repo_id))
        .filter(flocks::slug.eq(flock_slug))
        .select(FlockRow::as_select())
        .first::<FlockRow>(conn)
        .optional()?
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "flock `{flock_slug}` does not exist under repo `{repo_url}`"
            ))
        })
}

pub fn fetch_owner(conn: &mut PgConnection, user_id: Uuid) -> Result<UserRow, AppError> {
    users::table
        .find(user_id)
        .select(UserRow::as_select())
        .first::<UserRow>(conn)
        .map_err(Into::into)
}

pub fn load_users_map(
    conn: &mut PgConnection,
    ids: Vec<Uuid>,
) -> Result<HashMap<Uuid, UserRow>, AppError> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = users::table
        .filter(users::id.eq_any(ids))
        .select(UserRow::as_select())
        .load::<UserRow>(conn)?;
    Ok(rows.into_iter().map(|row| (row.id, row)).collect())
}
pub fn load_skill_versions_map(
    conn: &mut PgConnection,
    ids: Vec<Uuid>,
) -> Result<HashMap<Uuid, SkillVersionRow>, AppError> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = skill_versions::table
        .filter(skill_versions::id.eq_any(ids))
        .select(SkillVersionRow::as_select())
        .load::<SkillVersionRow>(conn)?;
    Ok(rows.into_iter().map(|row| (row.id, row)).collect())
}

pub fn fetch_skill_versions(
    conn: &mut PgConnection,
    skill_id: Uuid,
    viewer: Option<&RequestUser>,
) -> Result<Vec<SkillVersionRow>, AppError> {
    let mut query = skill_versions::table
        .filter(skill_versions::skill_id.eq(skill_id))
        .into_boxed();
    if !viewer_is_staff(viewer) {
        query = query.filter(skill_versions::soft_deleted_at.is_null());
    }
    query
        .order(skill_versions::created_at.desc())
        .select(SkillVersionRow::as_select())
        .load::<SkillVersionRow>(conn)
        .map_err(Into::into)
}

pub fn user_summary_from_row(row: &UserRow) -> UserSummary {
    UserSummary {
        id: row.id,
        handle: row.handle.clone(),
        display_name: row.display_name.clone(),
        avatar_url: row.avatar_url.clone(),
        role: parse_role(&row.role),
    }
}

pub fn skill_item_from_rows(
    row: &SkillRow,
    repo_url: &str,
    owner: &UserRow,
    latest: Option<&SkillVersionRow>,
) -> SkillListItem {
    SkillListItem {
        id: row.id,
        slug: row.slug.clone(),
        path: row.path.clone(),
        display_name: row.name.clone(),
        summary: row.description.clone(),
        repo_id: row.repo_id.to_string(),
        owner: user_summary_from_row(owner),
        tags: parse_tag_map(&row.tags),
        stats: CatalogStats {
            downloads: row.stats_downloads,
            stars: row.stats_stars,
            versions: row.stats_versions,
            comments: row.stats_comments,
            installs: row.stats_installs,
            unique_users: row.stats_unique_users,
        },
        badges: SkillBadges {
            highlighted: row.highlighted,
            official: row.official,
            deprecated: row.deprecated,
            suspicious: row.suspicious,
        },
        moderation_status: parse_moderation_status(&row.moderation_status),
        security_status: super::security::parse_security_status(&row.security_status),
        created_at: row.created_at,
        updated_at: row.updated_at,
        latest_version: latest.map(version_summary_from_skill),
    }
}

pub fn version_summary_from_skill(row: &SkillVersionRow) -> VersionSummary {
    VersionSummary {
        id: row.id,
        version: row.version.clone().unwrap_or_default(),
        changelog: row.changelog.clone(),
        tags: row.tags.iter().filter_map(|t| t.clone()).collect(),
        created_at: row.created_at,
        scan_summary: parse_scan_summary(&row.scan_summary),
    }
}

fn parse_scan_summary(value: &Option<Value>) -> Option<VersionScanSummary> {
    value
        .as_ref()
        .and_then(|v| serde_json::from_value::<VersionScanSummary>(v.clone()).ok())
}

pub fn version_detail_from_skill(row: &SkillVersionRow) -> Result<VersionDetail, AppError> {
    let files = parse_files(&row.files)?;
    let markdown = select_markdown_file(&files, "SKILL.md");
    Ok(VersionDetail {
        id: row.id,
        version: row.version.clone().unwrap_or_default(),
        changelog: row.changelog.clone(),
        tags: row.tags.iter().filter_map(|t| t.clone()).collect(),
        created_at: row.created_at,
        files: files
            .iter()
            .map(|file| ResourceFileSummary {
                path: file.path.clone(),
                size: file.size,
                sha256: file.sha256.clone(),
            })
            .collect(),
        markdown_html: render_markdown(
            markdown
                .map(|file| file.content.as_str())
                .unwrap_or_default(),
        ),
        parsed_metadata: row.parsed_metadata.clone(),
        bundle_metadata: bundle_metadata_from_json(&row.parsed_metadata).ok(),
        scan_summary: parse_scan_summary(&row.scan_summary),
    })
}

pub fn resolve_latest_skill_version<'a>(
    row: &SkillRow,
    versions: &'a [SkillVersionRow],
) -> Option<&'a SkillVersionRow> {
    row.latest_version_id
        .and_then(|id| versions.iter().find(|version| version.id == id))
        .or_else(|| versions.first())
}

pub fn locate_skill_version(
    conn: &mut PgConnection,
    skill: &SkillRow,
    version: Option<&str>,
    tag: Option<&str>,
    viewer: Option<&RequestUser>,
) -> Result<SkillVersionRow, AppError> {
    if let Some(version) = version {
        return skill_versions::table
            .filter(skill_versions::skill_id.eq(skill.id))
            .filter(skill_versions::version.eq(version))
            .select(SkillVersionRow::as_select())
            .first::<SkillVersionRow>(conn)
            .map_err(Into::into);
    }
    if let Some(tag) = tag {
        let tags = parse_tag_map(&skill.tags);
        let version = tags
            .get(tag)
            .ok_or_else(|| AppError::NotFound(format!("tag `{tag}` not found")))?;
        return locate_skill_version(conn, skill, Some(version), None, viewer);
    }
    fetch_skill_versions(conn, skill.id, viewer)?
        .into_iter()
        .next()
        .ok_or_else(|| AppError::NotFound("no versions available".to_string()))
}

pub fn parse_files(value: &Value) -> Result<Vec<StoredBundleFile>, AppError> {
    serde_json::from_value(value.clone()).map_err(|error| AppError::Internal(error.to_string()))
}

pub fn parse_tag_map(value: &Value) -> indexmap::IndexMap<String, String> {
    serde_json::from_value(value.clone()).unwrap_or_default()
}

pub fn parse_frontmatter(markdown: &str) -> Value {
    let markdown = markdown.trim_start_matches('\u{feff}');
    if !markdown.starts_with("---") {
        return Value::Object(Map::new());
    }
    let remainder = &markdown[3..];
    let Some((yaml, _body)) = remainder.split_once("\n---") else {
        return Value::Object(Map::new());
    };
    serde_saphyr::from_str::<Value>(yaml.trim()).unwrap_or_else(|_| Value::Object(Map::new()))
}

pub fn extract_summary(parsed: &Value, markdown: &str) -> Option<String> {
    if let Some(summary) = parsed.get("description").and_then(Value::as_str) {
        let summary = summary.trim();
        if !summary.is_empty() {
            return Some(summary.to_string());
        }
    }
    markdown
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("---") && !line.starts_with('#'))
        .map(ToString::to_string)
}

pub fn hash_string(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn select_markdown_file<'a>(
    files: &'a [StoredBundleFile],
    preferred: &str,
) -> Option<&'a StoredBundleFile> {
    files
        .iter()
        .find(|file| file.path.eq_ignore_ascii_case(preferred))
        .or_else(|| files.iter().find(|file| file.path.ends_with(".md")))
}

pub fn score_text(
    query: &str,
    slug: &str,
    display_name: &str,
    summary: &str,
    content: &str,
) -> Option<f32> {
    let tokens = query.split_whitespace().collect::<Vec<_>>();
    let haystack_slug = slug.to_lowercase();
    let haystack_name = display_name.to_lowercase();
    let haystack_summary = summary.to_lowercase();
    let haystack_content = content.to_lowercase();

    let mut score = 0.0;
    if haystack_slug == query {
        score += 100.0;
    }
    if haystack_slug.contains(query) {
        score += 45.0;
    }
    if haystack_name.contains(query) {
        score += 30.0;
    }
    if haystack_summary.contains(query) {
        score += 18.0;
    }
    for token in tokens {
        if haystack_slug.contains(token) {
            score += 12.0;
        }
        if haystack_name.contains(token) {
            score += 8.0;
        }
        if haystack_summary.contains(token) {
            score += 5.0;
        }
        if haystack_content.contains(token) {
            score += 1.5;
        }
    }
    if score > 0.0 { Some(score) } else { None }
}

pub fn zip_files(files: &[StoredBundleFile]) -> Result<Vec<u8>, AppError> {
    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut cursor);
        let options = SimpleFileOptions::default();
        for file in files {
            writer
                .start_file(&file.path, options)
                .map_err(|error| AppError::Internal(error.to_string()))?;
            writer
                .write_all(file.content.as_bytes())
                .map_err(|error| AppError::Internal(error.to_string()))?;
        }
        writer
            .finish()
            .map_err(|error| AppError::Internal(error.to_string()))?;
    }
    Ok(cursor.into_inner())
}

pub fn viewer_is_staff(viewer: Option<&RequestUser>) -> bool {
    viewer
        .map(|viewer| matches!(viewer.role, UserRole::Admin | UserRole::Moderator))
        .unwrap_or(false)
}

pub fn viewer_is_admin(viewer: Option<&RequestUser>) -> bool {
    viewer
        .map(|viewer| matches!(viewer.role, UserRole::Admin))
        .unwrap_or(false)
}

pub fn ensure_skill_visible(row: &SkillRow, viewer: Option<&RequestUser>) -> Result<(), AppError> {
    let can_view_hidden = viewer
        .map(|viewer| matches!(viewer.role, UserRole::Admin | UserRole::Moderator))
        .unwrap_or(false);
    if row.soft_deleted_at.is_some() || row.moderation_status == "removed" {
        if !can_view_hidden {
            return Err(AppError::NotFound(format!(
                "skill `{}` does not exist",
                row.slug
            )));
        }
    }
    if row.moderation_status == "hidden" && !can_view_hidden {
        return Err(AppError::NotFound(format!(
            "skill `{}` does not exist",
            row.slug
        )));
    }
    Ok(())
}

pub fn moderation_status_to_str(status: ModerationStatus) -> &'static str {
    match status {
        ModerationStatus::Active => "active",
        ModerationStatus::Hidden => "hidden",
        ModerationStatus::Removed => "removed",
    }
}

pub fn parse_moderation_status(value: &str) -> ModerationStatus {
    match value {
        "hidden" => ModerationStatus::Hidden,
        "removed" => ModerationStatus::Removed,
        _ => ModerationStatus::Active,
    }
}

pub fn insert_audit_log(
    conn: &mut PgConnection,
    actor_user_id: Option<Uuid>,
    action: &str,
    target_type: &str,
    target_id: Option<Uuid>,
    metadata: Value,
) -> Result<(), AppError> {
    diesel::insert_into(audit_logs::table)
        .values(NewAuditLogRow {
            id: Uuid::now_v7(),
            actor_user_id,
            action: action.to_string(),
            target_type: target_type.to_string(),
            target_id,
            metadata,
            created_at: Utc::now(),
        })
        .execute(conn)?;
    Ok(())
}

pub fn audit_log_entry_from_row(
    row: AuditLogRow,
    actors: &HashMap<Uuid, UserRow>,
) -> AuditLogEntry {
    AuditLogEntry {
        id: row.id,
        action: row.action,
        target_type: row.target_type,
        target_id: row.target_id,
        actor: row
            .actor_user_id
            .and_then(|id| actors.get(&id))
            .map(user_summary_from_row),
        metadata: row.metadata,
        created_at: row.created_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_git_url_ssh_to_https() {
        assert_eq!(
            normalize_git_url("git@github.com:anthropics/skills.git"),
            "https://github.com/anthropics/skills.git"
        );
        assert_eq!(
            normalize_git_url("git@github.com:anthropics/skills"),
            "https://github.com/anthropics/skills.git"
        );
    }

    #[test]
    fn normalize_git_url_adds_git_suffix() {
        assert_eq!(
            normalize_git_url("https://github.com/org/repo"),
            "https://github.com/org/repo.git"
        );
        assert_eq!(
            normalize_git_url("https://github.com/org/repo.git"),
            "https://github.com/org/repo.git"
        );
    }

    #[test]
    fn normalize_git_url_upgrades_http() {
        assert_eq!(
            normalize_git_url("http://github.com/org/repo.git"),
            "https://github.com/org/repo.git"
        );
    }

    #[test]
    fn normalize_git_url_strips_trailing_slash() {
        assert_eq!(
            normalize_git_url("https://github.com/org/repo/"),
            "https://github.com/org/repo.git"
        );
        assert_eq!(
            normalize_git_url("https://github.com/org/repo.git/"),
            "https://github.com/org/repo.git"
        );
    }

    #[test]
    fn parse_git_url_parts_extracts_domain_and_path_slug() {
        let (domain, slug) = parse_git_url_parts("https://github.com/anthropics/skills.git");
        assert_eq!(domain, "github.com");
        assert_eq!(slug, "anthropics/skills");
    }

    #[test]
    fn parse_git_url_parts_handles_deep_paths() {
        let (domain, slug) = parse_git_url_parts("https://github.com/mofa-org/mofa-skills.git");
        assert_eq!(domain, "github.com");
        assert_eq!(slug, "mofa-org/mofa-skills");
    }

    #[test]
    fn parse_git_url_parts_handles_port() {
        let (domain, slug) = parse_git_url_parts("https://git.example.com:8443/org/repo.git");
        assert_eq!(domain, "git.example.com-8443");
        assert_eq!(slug, "org/repo");
    }

    #[test]
    fn normalize_then_parse_is_consistent() {
        let inputs = [
            "git@github.com:anthropics/skills.git",
            "git@github.com:anthropics/skills",
            "https://github.com/anthropics/skills.git",
            "https://github.com/anthropics/skills",
            "https://github.com/anthropics/skills/",
            "http://github.com/anthropics/skills.git",
        ];
        for input in inputs {
            let normalized = normalize_git_url(input);
            let (domain, slug) = parse_git_url_parts(&normalized);
            assert_eq!(domain, "github.com", "domain mismatch for input: {input}");
            assert_eq!(
                slug, "anthropics/skills",
                "slug mismatch for input: {input}"
            );
        }
    }
}
