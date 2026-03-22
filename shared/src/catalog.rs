use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use semver::Version;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{CommentDto, FlockRatingStats, SecurityStatus, UserSummary};

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

fn is_default_security(s: &SecuritySummary) -> bool {
    *s == SecuritySummary::default()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistryVisibility {
    Public,
    Unlisted,
    Private,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistryStatus {
    Draft,
    Active,
    Experimental,
    Deprecated,
    Archived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeSandbox {
    None,
    Preferred,
    Required,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryMaintainer {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatibilityMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub savfox: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub platforms: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub architectures: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CatalogSource {
    Registry { path: String },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeMetadata {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bins: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<RuntimeSandbox>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_mb: Option<i32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoMetadata {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub maintainers: Vec<RegistryMaintainer>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlockMetadata {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub maintainers: Vec<RegistryMaintainer>,
    #[serde(default, skip_serializing_if = "is_default_compatibility")]
    pub compatibility: CompatibilityMetadata,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub featured_skills: Vec<String>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub links: IndexMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportedSkillMetadata {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub maintainers: Vec<RegistryMaintainer>,
    #[serde(default, skip_serializing_if = "is_default_compatibility")]
    pub compatibility: CompatibilityMetadata,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub links: IndexMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoDocument {
    pub name: String,
    pub description: String,
    pub git_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,
    #[serde(default = "default_visibility")]
    pub visibility: RegistryVisibility,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub verified: bool,
    #[serde(flatten)]
    pub metadata: RepoMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlockDocument {
    pub repo: String,
    pub name: String,
    pub description: String,
    /// Subdirectory path within the git repo where skills are located.
    /// `None` means the repository root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub status: RegistryStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<RegistryVisibility>,
    pub license: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<CatalogSource>,
    /// Automated security scan results.
    #[serde(default, skip_serializing_if = "is_default_security")]
    pub security: SecuritySummary,
    #[serde(flatten)]
    pub metadata: FlockMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportedSkillRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<uuid::Uuid>,
    pub slug: String,
    /// Path to the skill directory relative to the git repo root (e.g. "skills/salvo-auth").
    pub path: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub status: RegistryStatus,
    pub license: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<RuntimeMetadata>,
    /// Automated security scan results.
    #[serde(default, skip_serializing_if = "is_default_security")]
    pub security: SecuritySummary,
    #[serde(flatten)]
    pub metadata: ImportedSkillMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateRepoRequest {
    pub git_url: String,
    pub name: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportFlockRequest {
    pub slug: String,
    #[serde(flatten)]
    pub document: FlockDocument,
    #[serde(default)]
    pub skills: Vec<ImportedSkillRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepoSummary {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub git_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,
    pub visibility: RegistryVisibility,
    pub verified: bool,
    pub flock_count: i64,
    pub skill_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlockSummary {
    pub id: Uuid,
    pub repo_url: String,
    pub slug: String,
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub status: RegistryStatus,
    pub visibility: Option<RegistryVisibility>,
    pub license: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<CatalogSource>,
    pub imported_by: UserSummary,
    pub skill_count: i64,
    pub rating: FlockRatingStats,
    pub stats_comments: i64,
    pub stats_stars: i64,
    #[serde(default)]
    pub stats_max_installs: i64,
    #[serde(default)]
    pub stats_max_unique_users: i64,
    pub security_status: SecurityStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepoDetailResponse {
    pub repo: RepoSummary,
    pub document: RepoDocument,
    pub flocks: Vec<FlockSummary>,
    pub skills: Vec<ImportedSkillRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlockDetailResponse {
    pub flock: FlockSummary,
    pub document: FlockDocument,
    pub skills: Vec<ImportedSkillRecord>,
    pub comments: Vec<CommentDto>,
    pub user_rating: Option<i16>,
    #[serde(default)]
    pub starred: bool,
}

fn default_visibility() -> RegistryVisibility {
    RegistryVisibility::Public
}

fn is_default_compatibility(c: &CompatibilityMetadata) -> bool {
    *c == CompatibilityMetadata::default()
}

pub fn is_registry_slug(value: &str) -> bool {
    let value = value.trim();
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes.iter().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-' || *byte == b'_'
        })
}

fn is_safe_relative_path(value: &str) -> bool {
    let value = value.trim();
    if value == "." {
        return true;
    }
    if value.is_empty()
        || value.starts_with('/')
        || value.contains('\\')
        || looks_like_windows_drive_path(value)
    {
        return false;
    }
    value
        .split('/')
        .all(|part| !part.is_empty() && part != "." && part != "..")
}

pub fn validate_repo_document(document: &RepoDocument) -> Result<(), String> {
    if document.name.trim().is_empty() {
        return Err("repo name is required".to_string());
    }
    if document.git_url.trim().is_empty() {
        return Err("repo git_url is required".to_string());
    }
    validate_maintainers("repo.maintainers", &document.metadata.maintainers)?;
    Ok(())
}

pub fn validate_flock_document(document: &FlockDocument) -> Result<(), String> {
    if document.repo.trim().is_empty() {
        return Err("flock repo is required".to_string());
    }
    if document.name.trim().is_empty() {
        return Err("flock name is required".to_string());
    }
    if document.description.trim().is_empty() {
        return Err("flock description is required".to_string());
    }
    if document.license.trim().is_empty() {
        return Err("flock license is required".to_string());
    }
    if let Some(ref v) = document.version {
        Version::parse(v.trim()).map_err(|_| "flock version must be valid semver".to_string())?;
    }
    validate_source("flock.source", document.source.as_ref())?;
    validate_maintainers("flock.maintainers", &document.metadata.maintainers)?;
    validate_links("flock.links", &document.metadata.links)?;
    validate_compatibility(
        "flock.compatibility",
        Some(&document.metadata.compatibility),
    )?;
    Ok(())
}

pub fn validate_imported_skill_record(
    _repo_id: &str,
    _flock_slug: &str,
    skill: &ImportedSkillRecord,
) -> Result<(), String> {
    if !is_registry_slug(&skill.slug) {
        return Err(format!(
            "skill slug `{}` must match the registry skill slug format",
            skill.slug
        ));
    }
    if skill.name.trim().is_empty() {
        return Err(format!("skill `{}` name is required", skill.slug));
    }
    if skill.license.trim().is_empty() {
        return Err(format!("skill `{}` license is required", skill.slug));
    }
    if let Some(ref v) = skill.version {
        Version::parse(v.trim())
            .map_err(|_| format!("skill `{}` version must be valid semver", skill.slug))?;
    }
    if let Some(runtime) = &skill.runtime {
        if runtime.memory_mb.is_some_and(|value| value <= 0) {
            return Err(format!(
                "skill `{}` runtime.memory_mb must be positive",
                skill.slug
            ));
        }
    }
    validate_maintainers(
        &format!("skill `{}`.maintainers", skill.slug),
        &skill.metadata.maintainers,
    )?;
    validate_links(
        &format!("skill `{}`.links", skill.slug),
        &skill.metadata.links,
    )?;
    validate_compatibility(
        &format!("skill `{}`.compatibility", skill.slug),
        Some(&skill.metadata.compatibility),
    )?;
    Ok(())
}

fn validate_source(label: &str, source: Option<&CatalogSource>) -> Result<(), String> {
    let Some(source) = source else {
        return Ok(());
    };
    let CatalogSource::Registry { path } = source;
    if !is_safe_relative_path(path) {
        return Err(format!("{label} path must be a safe relative path"));
    }
    Ok(())
}

fn validate_maintainers(label: &str, maintainers: &[RegistryMaintainer]) -> Result<(), String> {
    for (index, maintainer) in maintainers.iter().enumerate() {
        if maintainer.id.trim().is_empty() {
            return Err(format!("{label}[{index}].id is required"));
        }
        if maintainer.name.trim().is_empty() {
            return Err(format!("{label}[{index}].name is required"));
        }
    }
    Ok(())
}

fn validate_links(label: &str, links: &IndexMap<String, String>) -> Result<(), String> {
    for (key, value) in links {
        if key.trim().is_empty() || value.trim().is_empty() {
            return Err(format!(
                "{label} entries must use non-empty keys and values"
            ));
        }
    }
    Ok(())
}

fn validate_compatibility(
    label: &str,
    compatibility: Option<&CompatibilityMetadata>,
) -> Result<(), String> {
    let Some(compatibility) = compatibility else {
        return Ok(());
    };
    for value in compatibility
        .platforms
        .iter()
        .chain(compatibility.architectures.iter())
    {
        if value.trim().is_empty() {
            return Err(format!("{label} cannot contain empty values"));
        }
    }
    Ok(())
}

fn looks_like_windows_drive_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 2 && bytes[1] == b':'
}
