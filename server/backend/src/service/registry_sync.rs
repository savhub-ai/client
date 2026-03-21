use std::collections::HashSet;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use reqwest::Url;
use reqwest::header::{ACCEPT, USER_AGENT};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zip::ZipArchive;

use crate::error::AppError;
use crate::state::app_state;
use shared::{
    CatalogSource, FlockDocument, FlockMetadata, ImportedSkillMetadata, ImportedSkillRecord,
    RegistryGitReference, RegistryMaintainer,
};

use super::helpers::{extract_summary, parse_frontmatter};

const GITHUB_API_BASE: &str = "https://api.github.com/";

/// On Windows, configure the repo so that files with characters illegal in
/// Windows paths (e.g. `:`) are accepted in the index but excluded from the
/// worktree.  Two settings work together:
///
/// - `core.protectNTFS = false` — lets git put the entry in the index without
///   rejecting the path.
/// - sparse-checkout — marks those entries as skip-worktree so git never
///   attempts to write them to disk.
///
/// No-op on non-Windows.  Idempotent.
async fn setup_windows_sparse_checkout(repo_dir: &Path) {
    if !cfg!(windows) {
        return;
    }
    // Allow Windows-invalid paths in the index.
    let _ = tokio::process::Command::new("git")
        .args(["config", "core.protectNTFS", "false"])
        .current_dir(repo_dir)
        .output()
        .await;
    let _ = tokio::process::Command::new("git")
        .args(["config", "core.sparseCheckout", "true"])
        .current_dir(repo_dir)
        .output()
        .await;
    let info_dir = repo_dir.join(".git").join("info");
    let _ = fs::create_dir_all(&info_dir);
    // Non-cone sparse-checkout (gitignore syntax): include everything, then
    // exclude filenames containing ':' (and other Windows-illegal chars).
    let _ = fs::write(info_dir.join("sparse-checkout"), "*\n!*:*\n");
}

const GITHUB_ACCEPT: &str = "application/vnd.github+json";
const REGISTRY_SYNC_USER_AGENT: &str = "savhub-registry-sync";

#[derive(Debug, Clone)]
pub struct ScannedFlockImport {
    pub readme_path: Option<String>,
    pub skills: Vec<ImportedSkillRecord>,
}

#[derive(Debug, Clone)]
pub(crate) struct GithubRepoSpec {
    pub(crate) source_url: String,
    pub(crate) owner: String,
    pub(crate) repo: String,
    pub(crate) reference: RegistryGitReference,
    pub(crate) path: String,
}

#[derive(Debug, Clone)]
pub(crate) struct SkillCandidate {
    pub(crate) path: PathBuf,
    pub(crate) relative_dir: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ScannedSkillMetadata {
    pub(crate) slug: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) version: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct CachedRepoCheckout {
    pub(crate) path: PathBuf,
    pub(crate) head_sha: String,
    pub(crate) previous_sha: Option<String>,
    pub(crate) changed_skill_files: Vec<String>,
    pub(crate) reused: bool,
    /// If git followed an HTTP redirect (e.g. the repo was moved/renamed),
    /// this contains the new URL so callers can update their records.
    pub(crate) redirected_url: Option<String>,
}

#[derive(Debug)]
struct TempDirGuard {
    path: PathBuf,
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub async fn scan_github_flock_import(
    flock_slug: &str,
    document: &FlockDocument,
) -> Result<ScannedFlockImport, AppError> {
    let source = document.source.as_ref().ok_or_else(|| {
        AppError::BadRequest("flock source is required for GitHub import".to_string())
    })?;
    let github = parse_github_source(source)?;
    let archive = download_github_archive(&github).await?;
    let temp_dir = create_temp_dir()?;
    extract_zip_archive(&archive, &temp_dir.path)?;
    let checkout_root = resolve_checkout_root(&temp_dir.path)?;
    let scan_root = resolve_scan_root(&checkout_root, &github.path)?;
    let readme_path = detect_root_readme(&scan_root);
    let skills = scan_checkout_for_skills(&scan_root, &github, flock_slug, document)?;
    if skills.is_empty() {
        return Err(AppError::BadRequest(format!(
            "GitHub repo `{}` does not contain any valid skill directories under `{}`",
            github.source_url, github.path
        )));
    }
    Ok(ScannedFlockImport {
        readme_path,
        skills,
    })
}

/// When a git remote returns an HTTP redirect (301/302), the repo has been
/// moved or renamed.  This function updates the database (repos, flocks,
/// skills) so everything points to the new URL.
///
/// `old_url` and `new_url` should both be **normalized** HTTPS URLs.
pub(crate) fn apply_repo_redirect(
    conn: &mut diesel::PgConnection,
    repo_id: Uuid,
    old_url: &str,
    new_url: &str,
) -> Result<(), AppError> {
    use crate::models::RepoChangeset;
    use crate::schema::repos;
    use diesel::prelude::*;

    let new_url = super::helpers::normalize_git_url(new_url);
    let (new_domain, new_path_slug) = super::helpers::parse_git_url_parts(&new_url);
    let new_sign = format!("{new_domain}/{new_path_slug}");

    let old_url_normalized = super::helpers::normalize_git_url(old_url);
    let (old_domain, old_path_slug) = super::helpers::parse_git_url_parts(&old_url_normalized);
    let old_sign = format!("{old_domain}/{old_path_slug}");

    if old_sign == new_sign {
        // The normalized sign didn't actually change (e.g. only .git suffix difference).
        return Ok(());
    }

    tracing::info!(
        repo_id = %repo_id,
        old_sign = old_sign.as_str(),
        new_sign = new_sign.as_str(),
        "applying repo redirect: updating DB"
    );

    // 1. Update repos table: git_url + sign
    diesel::update(repos::table.find(repo_id))
        .set(RepoChangeset {
            sign: Some(new_sign.clone()),
            git_url: Some(new_url.clone()),
            updated_at: Some(chrono::Utc::now()),
            ..Default::default()
        })
        .execute(conn)
        .map_err(|e| {
            AppError::Internal(format!("failed to update repo URL after redirect: {e}"))
        })?;

    // 2. Update flocks table: sign prefix  (old_sign/slug → new_sign/slug)
    diesel::sql_query(
        "UPDATE flocks SET sign = REPLACE(sign, $1, $2), updated_at = NOW() \
         WHERE repo_id = $3 AND sign LIKE $4",
    )
    .bind::<diesel::sql_types::Text, _>(format!("{old_sign}/"))
    .bind::<diesel::sql_types::Text, _>(format!("{new_sign}/"))
    .bind::<diesel::sql_types::Uuid, _>(repo_id)
    .bind::<diesel::sql_types::Text, _>(format!("{old_sign}/%"))
    .execute(conn)
    .map_err(|e| AppError::Internal(format!("failed to update flock signs after redirect: {e}")))?;

    // 3. Update flocks table: source JSON  (update the "url" field inside the JSONB)
    diesel::sql_query(
        "UPDATE flocks SET source = jsonb_set(source, '{url}', to_jsonb($1::text)) \
         WHERE repo_id = $2 AND source->>'kind' = 'git'",
    )
    .bind::<diesel::sql_types::Text, _>(&new_url)
    .bind::<diesel::sql_types::Uuid, _>(repo_id)
    .execute(conn)
    .map_err(|e| {
        AppError::Internal(format!(
            "failed to update flock source URLs after redirect: {e}"
        ))
    })?;

    // 4. Update skills table: sign prefix  (old_sign/… → new_sign/…)
    diesel::sql_query(
        "UPDATE skills SET sign = REPLACE(sign, $1, $2), updated_at = NOW() \
         WHERE repo_id = $3 AND sign LIKE $4",
    )
    .bind::<diesel::sql_types::Text, _>(format!("{old_sign}/"))
    .bind::<diesel::sql_types::Text, _>(format!("{new_sign}/"))
    .bind::<diesel::sql_types::Uuid, _>(repo_id)
    .bind::<diesel::sql_types::Text, _>(format!("{old_sign}/%"))
    .execute(conn)
    .map_err(|e| AppError::Internal(format!("failed to update skill signs after redirect: {e}")))?;

    Ok(())
}

async fn download_github_archive(github: &GithubRepoSpec) -> Result<Vec<u8>, AppError> {
    let mut url = Url::parse(GITHUB_API_BASE)
        .map_err(|error| AppError::Internal(format!("failed to build GitHub API URL: {error}")))?;
    url.path_segments_mut()
        .map_err(|_| AppError::Internal("failed to build GitHub API path".to_string()))?
        .extend([
            "repos",
            github.owner.as_str(),
            github.repo.as_str(),
            "zipball",
            github.reference.value.as_str(),
        ]);

    let response = reqwest::Client::new()
        .get(url)
        .header(ACCEPT, GITHUB_ACCEPT)
        .header(USER_AGENT, REGISTRY_SYNC_USER_AGENT)
        .send()
        .await
        .map_err(|error| {
            AppError::Internal(format!("failed to download GitHub archive: {error}"))
        })?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let detail = if body.trim().is_empty() {
            format!("GitHub archive request returned HTTP {}", status.as_u16())
        } else {
            format!("GitHub archive request failed: {body}")
        };
        return Err(AppError::BadRequest(detail));
    }

    response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(|error| AppError::Internal(format!("failed to read GitHub archive: {error}")))
}

fn parse_github_source(source: &CatalogSource) -> Result<GithubRepoSpec, AppError> {
    let CatalogSource::Git {
        url,
        reference,
        path,
        ..
    } = source
    else {
        return Err(AppError::BadRequest(
            "server-side flock scans require a git source".to_string(),
        ));
    };

    let parsed = Url::parse(url).map_err(|_| {
        AppError::BadRequest("flock source.url must be a valid GitHub URL".to_string())
    })?;
    if parsed.domain() != Some("github.com") {
        return Err(AppError::BadRequest(
            "server-side flock scans currently support github.com URLs only".to_string(),
        ));
    }
    let segments = parsed
        .path_segments()
        .map(|segments| segments.collect::<Vec<_>>())
        .unwrap_or_default();
    if segments.len() < 2 {
        return Err(AppError::BadRequest(
            "flock source.url must include the GitHub owner and repo".to_string(),
        ));
    }

    Ok(GithubRepoSpec {
        source_url: url.clone(),
        owner: segments[0].to_string(),
        repo: segments[1].trim_end_matches(".git").to_string(),
        reference: reference.clone(),
        path: path.clone().unwrap_or_else(|| ".".to_string()),
    })
}

fn create_temp_dir() -> Result<TempDirGuard, AppError> {
    let path = std::env::temp_dir().join(format!("savhub-import-{}", Uuid::now_v7().simple()));
    fs::create_dir_all(&path).map_err(|error| {
        AppError::Internal(format!(
            "failed to create a temporary import directory: {error}"
        ))
    })?;
    Ok(TempDirGuard { path })
}

fn extract_zip_archive(bytes: &[u8], target: &Path) -> Result<(), AppError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| AppError::BadRequest(format!("invalid GitHub archive: {error}")))?;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|error| AppError::BadRequest(format!("invalid archive entry: {error}")))?;
        let Some(name) = file.enclosed_name() else {
            continue;
        };
        let out_path = target.join(name);
        if file.is_dir() {
            fs::create_dir_all(&out_path).map_err(|error| {
                AppError::Internal(format!("failed to create archive directory: {error}"))
            })?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                AppError::Internal(format!(
                    "failed to create archive parent directory: {error}"
                ))
            })?;
        }
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .map_err(|error| AppError::BadRequest(format!("failed to extract archive: {error}")))?;
        fs::write(out_path, buffer).map_err(|error| {
            AppError::Internal(format!("failed to write archive file: {error}"))
        })?;
    }
    Ok(())
}

fn resolve_checkout_root(target: &Path) -> Result<PathBuf, AppError> {
    let mut entries = fs::read_dir(target)
        .map_err(|error| AppError::Internal(format!("failed to read extracted archive: {error}")))?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    if entries.len() == 1 {
        let entry = entries.remove(0);
        let metadata = entry.metadata().map_err(|error| {
            AppError::Internal(format!("failed to inspect extracted archive root: {error}"))
        })?;
        if metadata.is_dir() {
            return Ok(entry.path());
        }
    }
    Ok(target.to_path_buf())
}

fn resolve_scan_root(checkout_root: &Path, source_path: &str) -> Result<PathBuf, AppError> {
    let root = checkout_root
        .canonicalize()
        .map_err(|error| AppError::Internal(format!("failed to resolve archive root: {error}")))?;
    let resolved = if source_path == "." {
        root.clone()
    } else {
        root.join(source_path).canonicalize().map_err(|_| {
            AppError::BadRequest(format!(
                "flock source path `{source_path}` was not found in the GitHub archive"
            ))
        })?
    };
    if !resolved.starts_with(&root) {
        return Err(AppError::BadRequest(
            "flock source path resolved outside the downloaded archive".to_string(),
        ));
    }
    Ok(resolved)
}

fn detect_root_readme(scan_root: &Path) -> Option<String> {
    let entries = fs::read_dir(scan_root).ok()?;
    for entry in entries.flatten() {
        let metadata = entry.metadata().ok()?;
        if !metadata.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.eq_ignore_ascii_case("README.md") || name.eq_ignore_ascii_case("README") {
            return Some(name);
        }
    }
    None
}

fn scan_checkout_for_skills(
    scan_root: &Path,
    github: &GithubRepoSpec,
    flock_slug: &str,
    flock: &FlockDocument,
) -> Result<Vec<ImportedSkillRecord>, AppError> {
    let mut candidates = Vec::new();
    collect_skill_candidates(scan_root, ".", &mut candidates)?;
    let mut skills = Vec::new();
    let mut errors = Vec::new();
    let mut seen_slugs = HashSet::new();

    for candidate in candidates {
        let markdown = match fs::read_to_string(candidate.path.join("SKILL.md")) {
            Ok(raw) => raw,
            Err(error) => {
                errors.push(format!(
                    "failed to read `{}/SKILL.md`: {error}",
                    candidate.relative_dir
                ));
                continue;
            }
        };
        let metadata =
            match parse_skill_markdown_metadata(&candidate.relative_dir, flock_slug, &markdown) {
                Ok(metadata) => metadata,
                Err(error) => {
                    errors.push(error);
                    continue;
                }
            };

        // Apply skill name/slug formatting
        let skill_sign = format!(
            "github.com/{}/{}/{}",
            github.owner, github.repo, candidate.relative_dir
        );
        let formatted_name =
            crate::service::index_jobs::format_skill_name(&metadata.name, &skill_sign);
        let slug = crate::service::index_jobs::format_skill_slug(&formatted_name);
        if slug.is_empty() || !seen_slugs.insert(slug.clone()) {
            if !slug.is_empty() {
                errors.push(format!(
                    "duplicate skill slug `{}` detected in the GitHub archive",
                    slug
                ));
            }
            continue;
        }

        let formatted_metadata = ScannedSkillMetadata {
            slug,
            name: formatted_name,
            ..metadata
        };
        skills.push(build_imported_skill_record(
            github,
            flock,
            &candidate.relative_dir,
            &formatted_metadata,
        ));
    }

    if !errors.is_empty() {
        return Err(AppError::BadRequest(format!(
            "GitHub import scan failed:\n{}",
            errors.join("\n")
        )));
    }

    skills.sort_by(|left, right| left.slug.cmp(&right.slug));
    Ok(skills)
}

pub(crate) fn collect_skill_candidates(
    root: &Path,
    relative_dir: &str,
    out: &mut Vec<SkillCandidate>,
) -> Result<(), AppError> {
    let entries = fs::read_dir(root)
        .map_err(|error| AppError::Internal(format!("failed to walk extracted repo: {error}")))?;
    let mut subdirs = Vec::new();
    let mut has_skill_md = false;

    for entry in entries {
        let entry = entry.map_err(|error| {
            AppError::Internal(format!("failed to walk extracted repo: {error}"))
        })?;
        let name = entry.file_name().to_string_lossy().to_string();
        let metadata = entry.metadata().map_err(|error| {
            AppError::Internal(format!("failed to inspect extracted repo: {error}"))
        })?;
        if metadata.is_dir() {
            if should_skip_dir(&name) {
                continue;
            }
            subdirs.push((entry.path(), join_relative_dir(relative_dir, &name)));
            continue;
        }

        if metadata.is_file() && name == "SKILL.md" {
            has_skill_md = true;
        }
    }

    if has_skill_md {
        out.push(SkillCandidate {
            path: root.to_path_buf(),
            relative_dir: relative_dir.to_string(),
        });
        return Ok(());
    }

    // Always recurse into subdirectories to find all SKILL.md files
    for (path, relative) in subdirs {
        collect_skill_candidates(&path, &relative, out)?;
    }
    Ok(())
}

fn should_skip_dir(name: &str) -> bool {
    name.starts_with('.') || matches!(name, "node_modules" | "target" | ".git" | ".savhub")
}

fn join_relative_dir(base: &str, child: &str) -> String {
    if base == "." {
        child.to_string()
    } else {
        format!("{base}/{child}")
    }
}

pub(crate) fn build_imported_skill_record(
    github: &GithubRepoSpec,
    flock: &FlockDocument,
    skill_dir: &str,
    metadata: &ScannedSkillMetadata,
) -> ImportedSkillRecord {
    ImportedSkillRecord {
        id: None,
        slug: metadata.slug.clone(),
        path: skill_dir.to_string(),
        name: metadata.name.clone(),
        description: Some(metadata.description.clone()),
        version: metadata.version.clone().or_else(|| flock.version.clone()),
        status: shared::RegistryStatus::Active,
        license: flock.license.clone(),
        runtime: None,
        security: shared::SecuritySummary::default(),
        metadata: ImportedSkillMetadata {
            categories: flock.metadata.categories.clone(),
            keywords: flock.metadata.keywords.clone(),
            maintainers: skill_maintainers(&flock.metadata, github, &metadata.slug),
            compatibility: flock.metadata.compatibility.clone(),
            links: flock.metadata.links.clone(),
        },
    }
}

fn skill_maintainers(
    flock_metadata: &FlockMetadata,
    github: &GithubRepoSpec,
    skill_slug: &str,
) -> Vec<RegistryMaintainer> {
    if !flock_metadata.maintainers.is_empty() {
        return flock_metadata.maintainers.clone();
    }
    vec![RegistryMaintainer {
        id: format!("{skill_slug}-github-owner"),
        name: github.owner.clone(),
        role: Some("maintainer".to_string()),
        email: None,
        url: Some(format!("https://github.com/{}", github.owner)),
    }]
}

pub(crate) fn parse_skill_markdown_metadata(
    skill_dir: &str,
    flock_slug: &str,
    markdown: &str,
) -> Result<ScannedSkillMetadata, String> {
    let parsed = parse_frontmatter(markdown);
    let name = parsed
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| extract_markdown_heading(markdown))
        .ok_or_else(|| {
            format!(
                "`{skill_dir}` must define a non-empty `name` in SKILL.md frontmatter or a markdown heading"
            )
        })?;
    let description = extract_summary(&parsed, markdown)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            format!(
                "`{skill_dir}` must define a non-empty `description` in SKILL.md frontmatter or body"
            )
        })?;
    let slug = derive_skill_slug(skill_dir, flock_slug, &name, &parsed).ok_or_else(|| {
        format!(
            "`{skill_dir}` did not produce a valid skill slug from its directory name or SKILL.md"
        )
    })?;
    let version = parsed
        .get("version")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToString::to_string);

    Ok(ScannedSkillMetadata {
        slug,
        name,
        description,
        version,
    })
}

fn derive_skill_slug(
    skill_dir: &str,
    flock_slug: &str,
    name: &str,
    parsed: &Value,
) -> Option<String> {
    let frontmatter_slug = parsed
        .get("slug")
        .and_then(Value::as_str)
        .map(sanitize_registry_slug)
        .filter(|value| !value.is_empty());
    if frontmatter_slug.is_some() {
        return frontmatter_slug;
    }

    let directory_slug = if skill_dir == "." {
        sanitize_registry_slug(flock_slug)
    } else {
        skill_dir
            .rsplit('/')
            .next()
            .map(sanitize_registry_slug)
            .unwrap_or_default()
    };
    if !directory_slug.is_empty() {
        return Some(directory_slug);
    }

    let fallback_slug = sanitize_registry_slug(name);
    if fallback_slug.is_empty() {
        None
    } else {
        Some(fallback_slug)
    }
}

pub(crate) fn sanitize_registry_slug(value: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;

    for ch in value.trim().chars().flat_map(char::to_lowercase) {
        let keep = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        if keep {
            slug.push(ch);
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }

    while slug.starts_with('-') {
        slug.remove(0);
    }
    while slug.ends_with('-') {
        slug.pop();
    }

    slug
}

fn repo_cache_dir_name(url: &str, git_ref: &str) -> String {
    let repo_name = repo_name_from_url(url);
    let ref_slug = sanitize_registry_slug(git_ref);
    let ref_part = if ref_slug.is_empty() {
        "head"
    } else {
        &ref_slug
    };
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    hasher.update(b"\n");
    hasher.update(git_ref.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    format!("{repo_name}-{ref_part}-{}", &hash[..12])
}

fn repo_name_from_url(url: &str) -> String {
    url.trim_end_matches('/')
        .trim_end_matches(".git")
        .rsplit('/')
        .next()
        .map(sanitize_registry_slug)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "repo".to_string())
}

fn skill_markdown_path(relative_dir: &str) -> String {
    if relative_dir == "." {
        "SKILL.md".to_string()
    } else {
        format!("{relative_dir}/SKILL.md")
    }
}

fn is_skill_markdown_path(path: &str) -> bool {
    path == "SKILL.md" || path.ends_with("/SKILL.md")
}

fn parse_changed_skill_markdown_paths(stdout: &str) -> Vec<String> {
    let mut paths = HashSet::new();

    for line in stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let columns = line.split('\t').collect::<Vec<_>>();
        let file_columns = match columns.as_slice() {
            [_, path] => vec![*path],
            [_, old_path, new_path] => vec![*old_path, *new_path],
            _ => continue,
        };

        for path in file_columns {
            if is_skill_markdown_path(path) {
                paths.insert(path.replace('\\', "/"));
            }
        }
    }

    let mut paths = paths.into_iter().collect::<Vec<_>>();
    paths.sort();
    paths
}

fn collect_skill_markdown_files(repo_dir: &Path) -> Result<Vec<String>, AppError> {
    let mut candidates = Vec::new();
    collect_skill_candidates(repo_dir, ".", &mut candidates)?;
    let mut files = candidates
        .into_iter()
        .map(|candidate| skill_markdown_path(&candidate.relative_dir))
        .collect::<Vec<_>>();
    files.sort();
    Ok(files)
}

pub(crate) async fn changed_skill_markdown_files(
    repo_dir: &Path,
    previous_sha: &str,
    current_sha: &str,
) -> Result<Vec<String>, AppError> {
    if previous_sha == current_sha {
        return Ok(Vec::new());
    }

    let output = tokio::process::Command::new("git")
        .args([
            "diff",
            "--name-status",
            "--find-renames",
            previous_sha,
            current_sha,
            "--",
        ])
        .current_dir(repo_dir)
        .output()
        .await
        .map_err(|error| AppError::Internal(format!("failed to run git diff: {error}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Internal(format!("git diff failed: {stderr}")));
    }

    Ok(parse_changed_skill_markdown_paths(
        &String::from_utf8_lossy(&output.stdout),
    ))
}

pub(crate) fn cached_repo_dir(base_path: &Path, url: &str, git_ref: &str) -> PathBuf {
    base_path.join(repo_cache_dir_name(url, git_ref))
}

pub(crate) async fn refresh_cached_repo(
    base_path: &Path,
    url: &str,
    git_ref: &str,
) -> Result<CachedRepoCheckout, AppError> {
    fs::create_dir_all(base_path).map_err(|error| {
        AppError::Internal(format!(
            "failed to create repo cache directory `{}`: {error}",
            base_path.display()
        ))
    })?;

    let repo_dir = cached_repo_dir(base_path, url, git_ref);

    // Acquire a per-repo lock so that concurrent callers for the same repo
    // wait for the clone/pull to finish instead of racing.
    let lock = app_state().repo_checkout_lock(&repo_dir);
    let _guard = lock.lock().await;

    let git_dir = repo_dir.join(".git");

    if repo_dir.exists() && !git_dir.is_dir() {
        tracing::warn!(
            "cached repo path `{}` exists but is not a git checkout, removing and re-cloning",
            repo_dir.display()
        );
        fs::remove_dir_all(&repo_dir).map_err(|error| {
            AppError::Internal(format!(
                "failed to remove broken repo cache `{}`: {error}",
                repo_dir.display()
            ))
        })?;
    }

    if git_dir.is_dir() {
        let previous_sha = get_head_sha(&repo_dir).await?;
        let pull_result = pull_git_repo(&repo_dir, git_ref).await?;
        let changed_skill_files =
            changed_skill_markdown_files(&repo_dir, &previous_sha, &pull_result.head_sha).await?;
        return Ok(CachedRepoCheckout {
            path: repo_dir,
            head_sha: pull_result.head_sha,
            previous_sha: Some(previous_sha),
            changed_skill_files,
            reused: true,
            redirected_url: pull_result.redirected_url,
        });
    }

    let clone_result = clone_git_repo(url, git_ref, &repo_dir).await?;
    let changed_skill_files = collect_skill_markdown_files(&repo_dir)?;
    Ok(CachedRepoCheckout {
        path: repo_dir,
        head_sha: clone_result.head_sha,
        previous_sha: None,
        changed_skill_files,
        reused: false,
        redirected_url: clone_result.redirected_url,
    })
}

fn extract_markdown_heading(markdown: &str) -> Option<String> {
    markdown.lines().find_map(|line| {
        let trimmed = line.trim();
        let title = trimmed.trim_start_matches('#').trim();
        if trimmed.starts_with('#') && !title.is_empty() {
            Some(title.to_string())
        } else {
            None
        }
    })
}

/// Result of a git clone or pull operation.
pub(crate) struct GitOpResult {
    pub head_sha: String,
    /// If git followed an HTTP redirect (repo moved/renamed), the new URL.
    pub redirected_url: Option<String>,
}

/// Parse git stderr for redirect warnings (HTTP 301/302).
/// Git outputs: `warning: redirecting to <url>`
fn parse_git_redirect(stderr: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(stderr);
    for line in text.lines() {
        if let Some(url) = line.strip_prefix("warning: redirecting to ") {
            let url = url.trim().trim_end_matches('/');
            if !url.is_empty() {
                return Some(url.to_string());
            }
        }
    }
    None
}

pub(crate) async fn clone_git_repo(
    url: &str,
    git_ref: &str,
    target_dir: &Path,
) -> Result<GitOpResult, AppError> {
    fs::create_dir_all(target_dir).map_err(|error| {
        AppError::Internal(format!(
            "failed to create clone target directory `{}`: {error}",
            target_dir.display()
        ))
    })?;
    let mut args = vec!["clone", "--depth", "1", "--single-branch"];
    // "HEAD" is not a valid branch name for --branch; omit it to clone the default branch.
    if !git_ref.is_empty() && !git_ref.eq_ignore_ascii_case("HEAD") {
        args.push("--branch");
        args.push(git_ref);
    }
    args.push(url);
    let lossy = target_dir.to_string_lossy();
    args.push(&lossy);
    let output = tokio::process::Command::new("git")
        .args(&args)
        .output()
        .await
        .map_err(|error| AppError::Internal(format!("failed to run git clone: {error}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // On Windows, filenames containing characters like ':' are invalid.
        // Git reports "Clone succeeded, but checkout failed" — the object
        // store is intact.  Configure sparse-checkout to exclude the
        // offending paths and retry.
        if cfg!(windows) && stderr.contains("Clone succeeded, but checkout failed") {
            tracing::warn!(
                "git clone checkout failed (invalid Windows filename), \
                 configuring sparse-checkout to skip them"
            );
            setup_windows_sparse_checkout(target_dir).await;
            let _ = tokio::process::Command::new("git")
                .args(["reset", "--hard", "HEAD"])
                .current_dir(target_dir)
                .output()
                .await;
        } else {
            return Err(AppError::BadRequest(format!("git clone failed: {stderr}")));
        }
    }

    let redirected_url = parse_git_redirect(&output.stderr);
    if let Some(ref new_url) = redirected_url {
        tracing::warn!(
            "git clone followed a redirect: {url} -> {new_url} (repo may have been moved/renamed)"
        );
    }

    let head_sha = get_head_sha(target_dir).await?;
    Ok(GitOpResult {
        head_sha,
        redirected_url,
    })
}

pub(crate) async fn pull_git_repo(repo_dir: &Path, git_ref: &str) -> Result<GitOpResult, AppError> {
    // Repo checkouts are disposable caches. Avoid merge-based pulls and
    // forcibly realign the worktree to the upstream branch.
    let fetch_args: Vec<&str> = if git_ref.is_empty() || git_ref.eq_ignore_ascii_case("HEAD") {
        vec!["fetch", "--prune", "origin"]
    } else {
        vec!["fetch", "--prune", "origin", git_ref]
    };

    let fetch_output = tokio::process::Command::new("git")
        .args(&fetch_args)
        .current_dir(repo_dir)
        .output()
        .await
        .map_err(|error| AppError::Internal(format!("failed to run git fetch: {error}")))?;

    if !fetch_output.status.success() {
        let stderr = String::from_utf8_lossy(&fetch_output.stderr);
        return Err(AppError::Internal(format!("git fetch failed: {stderr}")));
    }

    let redirected_url = parse_git_redirect(&fetch_output.stderr);
    if let Some(ref new_url) = redirected_url {
        tracing::warn!(
            "git fetch followed a redirect to {new_url} (repo may have been moved/renamed)"
        );
    }

    let reset_target = if git_ref.is_empty() || git_ref.eq_ignore_ascii_case("HEAD") {
        tokio::process::Command::new("git")
            .args(["symbolic-ref", "refs/remotes/origin/HEAD", "--short"])
            .current_dir(repo_dir)
            .output()
            .await
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| {
                let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if branch.is_empty() {
                    None
                } else {
                    Some(branch)
                }
            })
            .unwrap_or_else(|| "origin/main".to_string())
    } else {
        "FETCH_HEAD".to_string()
    };

    // On Windows, ensure sparse-checkout is configured so that files with
    // invalid characters (e.g. ':') are excluded before reset touches them.
    setup_windows_sparse_checkout(repo_dir).await;

    let reset_output = tokio::process::Command::new("git")
        .args(["reset", "--hard", &reset_target])
        .current_dir(repo_dir)
        .output()
        .await
        .map_err(|error| AppError::Internal(format!("failed to run git reset: {error}")))?;

    if !reset_output.status.success() {
        let stderr = String::from_utf8_lossy(&reset_output.stderr);
        // On Windows, tolerate "invalid path" errors — these are files with
        // characters like ':' that cannot exist on NTFS.  The rest of the
        // worktree is still updated correctly.
        if cfg!(windows) && stderr.contains("invalid path") {
            tracing::warn!("git reset had invalid-path warnings (non-fatal on Windows): {stderr}");
        } else {
            return Err(AppError::Internal(format!(
                "git reset --hard {reset_target} failed: {stderr}"
            )));
        }
    }

    let clean_output = tokio::process::Command::new("git")
        .args(["clean", "-ffdx"])
        .current_dir(repo_dir)
        .output()
        .await
        .map_err(|error| AppError::Internal(format!("failed to run git clean: {error}")))?;

    if !clean_output.status.success() {
        let stderr = String::from_utf8_lossy(&clean_output.stderr);
        return Err(AppError::Internal(format!("git clean failed: {stderr}")));
    }

    let head_sha = get_head_sha(repo_dir).await?;
    Ok(GitOpResult {
        head_sha,
        redirected_url,
    })
}

/// Resolve a remote ref to its commit SHA without cloning.
pub(crate) async fn resolve_remote_sha(url: &str, git_ref: &str) -> Result<String, AppError> {
    let output = tokio::process::Command::new("git")
        .args(["ls-remote", url, git_ref])
        .output()
        .await
        .map_err(|error| AppError::Internal(format!("failed to run git ls-remote: {error}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::BadRequest(format!(
            "git ls-remote failed: {stderr}"
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // ls-remote output: "<sha>\t<ref>\n"
    stdout
        .lines()
        .next()
        .and_then(|line| line.split('\t').next())
        .map(|sha| sha.trim().to_string())
        .filter(|sha| !sha.is_empty())
        .ok_or_else(|| AppError::BadRequest(format!("ref `{git_ref}` not found in remote repo")))
}

async fn get_head_sha(repo_dir: &Path) -> Result<String, AppError> {
    let output = tokio::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_dir)
        .output()
        .await
        .map_err(|error| AppError::Internal(format!("failed to get HEAD sha: {error}")))?;

    if !output.status.success() {
        return Err(AppError::Internal(
            "failed to get HEAD commit SHA".to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn collect_skill_candidates_finds_nested_skill_directories() {
        let root =
            std::env::temp_dir().join(format!("savhub-registry-sync-test-{}", Uuid::now_v7()));
        fs::create_dir_all(root.join("skills").join("python")).expect("create dirs");
        fs::write(
            root.join("skills").join("python").join("SKILL.md"),
            "---\nname: Python\ndescription: Python tools.\n---\n# Python",
        )
        .expect("write skill");

        let mut candidates = Vec::new();
        collect_skill_candidates(&root, ".", &mut candidates).expect("scan");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].relative_dir, "skills/python");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parse_skill_markdown_metadata_reads_frontmatter_without_meta_toml() {
        let markdown = "---\nname: Prompt Crafter\ndescription: Build prompts from templates.\n---\n# Prompt Crafter\n";

        let metadata =
            parse_skill_markdown_metadata("skills/prompt-crafter", "registry-tools", markdown)
                .expect("parse markdown");

        assert_eq!(metadata.slug, "prompt-crafter");
        assert_eq!(metadata.name, "Prompt Crafter");
        assert_eq!(metadata.description, "Build prompts from templates.");
    }

    #[test]
    fn parse_skill_markdown_metadata_falls_back_to_heading_and_body() {
        let markdown = "# Shell Runner\n\nExecute shell tasks safely.\n";

        let metadata =
            parse_skill_markdown_metadata(".", "shell-runner", markdown).expect("parse markdown");

        assert_eq!(metadata.slug, "shell-runner");
        assert_eq!(metadata.name, "Shell Runner");
        assert_eq!(metadata.description, "Execute shell tasks safely.");
    }

    #[test]
    fn cached_repo_dir_is_stable_per_url_and_ref() {
        let base = PathBuf::from("repos");
        let a = cached_repo_dir(&base, "https://github.com/acme/skills.git", "main");
        let b = cached_repo_dir(&base, "https://github.com/acme/skills.git", "main");
        let c = cached_repo_dir(&base, "https://github.com/acme/skills.git", "develop");

        assert_eq!(a, b);
        assert_ne!(a, c);
        assert!(a.to_string_lossy().contains("skills-main-"));
    }

    #[test]
    fn parse_changed_skill_markdown_paths_handles_edits_and_renames() {
        let stdout = "\
M\tskills/python/SKILL.md\n\
R100\ttools/old/SKILL.md\ttools/new/SKILL.md\n\
D\tREADME.md\n";

        let paths = parse_changed_skill_markdown_paths(stdout);

        assert_eq!(
            paths,
            vec![
                "skills/python/SKILL.md".to_string(),
                "tools/new/SKILL.md".to_string(),
                "tools/old/SKILL.md".to_string(),
            ]
        );
    }
}
