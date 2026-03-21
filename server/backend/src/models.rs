use chrono::{DateTime, Utc};
use diesel::{AsChangeset, Associations, Identifiable, Insertable, Queryable, Selectable};
use serde_json::Value;
use uuid::Uuid;

use crate::schema::{
    ai_request_cache, ai_usage_logs, audit_logs, flocks, index_jobs, index_rules, reports, repos,
    security_scans, site_admins, skill_blocks, skill_comments, skill_installs, skill_ratings,
    skill_stars, skill_versions, skills, browse_histories, user_tokens, users,
};

#[derive(Debug, Clone, Queryable, Selectable, Identifiable)]
#[diesel(table_name = users)]
pub struct UserRow {
    pub id: Uuid,
    pub handle: String,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
    pub github_user_id: Option<String>,
    pub github_login: Option<String>,
    pub role: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = users)]
pub struct NewUserRow {
    pub id: Uuid,
    pub handle: String,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
    pub github_user_id: Option<String>,
    pub github_login: Option<String>,
    pub role: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Default, Clone, AsChangeset)]
#[diesel(table_name = users)]
pub struct UserChangeset {
    pub handle: Option<String>,
    pub display_name: Option<Option<String>>,
    pub bio: Option<Option<String>>,
    pub avatar_url: Option<Option<String>>,
    pub github_user_id: Option<Option<String>>,
    pub github_login: Option<Option<String>>,
    pub role: Option<String>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, Associations)]
#[diesel(table_name = user_tokens)]
#[diesel(belongs_to(UserRow, foreign_key = user_id))]
pub struct UserTokenRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub token: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = user_tokens)]
pub struct NewUserTokenRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub token: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable)]
#[diesel(table_name = repos)]
pub struct RepoRow {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub git_url: String,
    pub git_rev: String,
    pub git_branch: Option<String>,
    pub license: Option<String>,
    pub visibility: String,
    pub verified: bool,
    pub metadata: Value,
    pub keywords: Vec<Option<String>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_indexed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = repos)]
pub struct NewRepoRow {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub git_url: String,
    pub git_rev: String,
    pub git_branch: Option<String>,
    pub license: Option<String>,
    pub visibility: String,
    pub verified: bool,
    pub metadata: Value,
    pub keywords: Vec<Option<String>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_indexed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Default, Clone, AsChangeset)]
#[diesel(table_name = repos)]
pub struct RepoChangeset {
    pub name: Option<String>,
    pub description: Option<String>,
    pub git_url: Option<String>,
    pub visibility: Option<String>,
    pub verified: Option<bool>,
    pub metadata: Option<Value>,
    pub updated_at: Option<DateTime<Utc>>,
    pub last_indexed_at: Option<Option<DateTime<Utc>>>,
    pub git_rev: Option<String>,
    pub git_branch: Option<Option<String>>,
    pub keywords: Option<Vec<Option<String>>>,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, Associations)]
#[diesel(table_name = flocks)]
#[diesel(belongs_to(RepoRow, foreign_key = repo_id))]
#[diesel(belongs_to(UserRow, foreign_key = imported_by_user_id))]
pub struct FlockRow {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub repo_id: Uuid,
    pub keywords: Vec<Option<String>>,
    pub description: String,
    pub version: Option<String>,
    pub status: String,
    pub visibility: Option<String>,
    pub license: Option<String>,
    pub metadata: Value,
    pub source: Value,
    pub imported_by_user_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub stats_comments: i64,
    pub stats_ratings: i64,
    pub stats_avg_rating: f64,
    pub security_status: String,
    pub stats_stars: i64,
    pub stats_max_installs: i64,
    pub stats_max_unique_users: i64,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = flocks)]
pub struct NewFlockRow {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub repo_id: Uuid,
    pub keywords: Vec<Option<String>>,
    pub description: String,
    pub version: Option<String>,
    pub status: String,
    pub visibility: Option<String>,
    pub license: Option<String>,
    pub metadata: Value,
    pub source: Value,
    pub imported_by_user_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub stats_comments: i64,
    pub stats_ratings: i64,
    pub stats_avg_rating: f64,
    pub security_status: String,
    pub stats_max_installs: i64,
    pub stats_max_unique_users: i64,
}

#[derive(Debug, Default, Clone, AsChangeset)]
#[diesel(table_name = flocks)]
pub struct FlockChangeset {
    pub name: Option<String>,
    pub keywords: Option<Vec<Option<String>>>,
    pub description: Option<String>,
    pub version: Option<String>,
    pub status: Option<String>,
    pub visibility: Option<Option<String>>,
    pub license: Option<String>,
    pub metadata: Option<Value>,
    pub source: Option<Value>,
    pub imported_by_user_id: Option<Uuid>,
    pub updated_at: Option<DateTime<Utc>>,
    pub stats_comments: Option<i64>,
    pub stats_ratings: Option<i64>,
    pub stats_avg_rating: Option<f64>,
    pub security_status: Option<String>,
    pub stats_max_installs: Option<i64>,
    pub stats_max_unique_users: Option<i64>,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, Associations)]
#[diesel(table_name = skills)]
#[diesel(belongs_to(FlockRow, foreign_key = flock_id))]
pub struct SkillRow {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub path: String,
    pub keywords: Vec<Option<String>>,
    pub description: Option<String>,
    pub repo_id: Uuid,
    pub flock_id: Uuid,
    pub version: Option<String>,
    pub status: String,
    pub license: Option<String>,
    pub source: Value,
    pub metadata: Value,
    pub entry_data: Option<Value>,
    pub runtime_data: Option<Value>,
    pub security_status: String,
    pub latest_version_id: Option<Uuid>,
    pub tags: Value,
    pub moderation_status: String,
    pub highlighted: bool,
    pub official: bool,
    pub deprecated: bool,
    pub suspicious: bool,
    pub stats_downloads: i64,
    pub stats_stars: i64,
    pub stats_versions: i64,
    pub stats_comments: i64,
    pub soft_deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub stats_installs: i64,
    pub stats_unique_users: i64,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = skills)]
pub struct NewSkillRow {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub path: String,
    pub keywords: Vec<Option<String>>,
    pub description: Option<String>,
    pub repo_id: Uuid,
    pub flock_id: Uuid,
    pub version: Option<String>,
    pub status: String,
    pub license: Option<String>,
    pub source: Value,
    pub metadata: Value,
    pub entry_data: Option<Value>,
    pub runtime_data: Option<Value>,
    pub security_status: String,
    pub latest_version_id: Option<Uuid>,
    pub tags: Value,
    pub moderation_status: String,
    pub highlighted: bool,
    pub official: bool,
    pub deprecated: bool,
    pub suspicious: bool,
    pub stats_downloads: i64,
    pub stats_stars: i64,
    pub stats_versions: i64,
    pub stats_comments: i64,
    pub soft_deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub stats_installs: i64,
    pub stats_unique_users: i64,
}

#[derive(Debug, Default, Clone, AsChangeset)]
#[diesel(table_name = skills)]
pub struct SkillChangeset {
    pub slug: Option<String>,
    pub name: Option<String>,
    pub path: Option<String>,
    pub description: Option<Option<String>>,
    pub keywords: Option<Vec<Option<String>>>,
    pub version: Option<String>,
    pub status: Option<String>,
    pub license: Option<String>,
    pub source: Option<Value>,
    pub metadata: Option<Value>,
    pub entry_data: Option<Option<Value>>,
    pub runtime_data: Option<Option<Value>>,
    pub security_status: Option<String>,
    pub latest_version_id: Option<Uuid>,
    pub tags: Option<Value>,
    pub moderation_status: Option<String>,
    pub highlighted: Option<bool>,
    pub official: Option<bool>,
    pub deprecated: Option<bool>,
    pub suspicious: Option<bool>,
    pub stats_downloads: Option<i64>,
    pub stats_stars: Option<i64>,
    pub stats_versions: Option<i64>,
    pub stats_comments: Option<i64>,
    pub stats_installs: Option<i64>,
    pub stats_unique_users: Option<i64>,
    pub soft_deleted_at: Option<Option<DateTime<Utc>>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable)]
#[diesel(table_name = skill_versions)]
pub struct SkillVersionRow {
    pub id: Uuid,
    pub repo_id: Uuid,
    pub flock_id: Option<Uuid>,
    pub skill_id: Option<Uuid>,
    pub git_rev: String,
    pub git_branch: String,
    pub version: Option<String>,
    pub changelog: String,
    pub tags: Vec<Option<String>>,
    pub files: Value,
    pub parsed_metadata: Value,
    pub search_document: String,
    pub fingerprint: String,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub soft_deleted_at: Option<DateTime<Utc>>,
    pub scan_summary: Option<Value>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = skill_versions)]
pub struct NewSkillVersionRow {
    pub id: Uuid,
    pub repo_id: Uuid,
    pub flock_id: Option<Uuid>,
    pub skill_id: Option<Uuid>,
    pub git_rev: String,
    pub git_branch: String,
    pub version: Option<String>,
    pub changelog: String,
    pub tags: Vec<Option<String>>,
    pub files: Value,
    pub parsed_metadata: Value,
    pub search_document: String,
    pub fingerprint: String,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub soft_deleted_at: Option<DateTime<Utc>>,
    pub scan_summary: Option<Value>,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable)]
#[diesel(table_name = skill_comments)]
pub struct SkillCommentRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub repo_id: Uuid,
    pub flock_id: Uuid,
    pub skill_id: Option<Uuid>,
    pub body: String,
    pub soft_deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = skill_comments)]
pub struct NewSkillCommentRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub repo_id: Uuid,
    pub flock_id: Uuid,
    pub skill_id: Option<Uuid>,
    pub body: String,
    pub soft_deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable)]
#[diesel(table_name = skill_stars)]
pub struct SkillStarRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub repo_id: Uuid,
    pub flock_id: Uuid,
    pub skill_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = skill_stars)]
pub struct NewSkillStarRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub repo_id: Uuid,
    pub flock_id: Uuid,
    pub skill_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable)]
#[diesel(table_name = skill_blocks)]
pub struct SkillBlockRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub repo_id: Uuid,
    pub flock_id: Uuid,
    pub skill_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = skill_blocks)]
pub struct NewSkillBlockRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub repo_id: Uuid,
    pub flock_id: Uuid,
    pub skill_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable)]
#[diesel(table_name = skill_installs)]
pub struct SkillInstallRow {
    pub id: Uuid,
    pub skill_id: Uuid,
    pub flock_id: Uuid,
    pub user_id: Option<Uuid>,
    pub client_type: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = skill_installs)]
pub struct NewSkillInstallRow {
    pub id: Uuid,
    pub skill_id: Uuid,
    pub flock_id: Uuid,
    pub user_id: Option<Uuid>,
    pub client_type: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable)]
#[diesel(table_name = skill_ratings)]
pub struct SkillRatingRow {
    pub id: Uuid,
    pub repo_id: Uuid,
    pub flock_id: Uuid,
    pub user_id: Uuid,
    pub score: i16,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = skill_ratings)]
pub struct NewSkillRatingRow {
    pub id: Uuid,
    pub repo_id: Uuid,
    pub flock_id: Uuid,
    pub user_id: Uuid,
    pub score: i16,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, Associations)]
#[diesel(table_name = browse_histories)]
#[diesel(belongs_to(UserRow, foreign_key = user_id))]
pub struct BrowseHistoryRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub resource_type: String,
    pub resource_id: Uuid,
    pub viewed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = browse_histories)]
pub struct NewBrowseHistoryRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub resource_type: String,
    pub resource_id: Uuid,
    pub viewed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, Associations)]
#[diesel(table_name = audit_logs)]
#[diesel(belongs_to(UserRow, foreign_key = actor_user_id))]
pub struct AuditLogRow {
    pub id: Uuid,
    pub actor_user_id: Option<Uuid>,
    pub action: String,
    pub target_type: String,
    pub target_id: Option<Uuid>,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = audit_logs)]
pub struct NewAuditLogRow {
    pub id: Uuid,
    pub actor_user_id: Option<Uuid>,
    pub action: String,
    pub target_type: String,
    pub target_id: Option<Uuid>,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable)]
#[diesel(table_name = reports)]
pub struct ReportRow {
    pub id: Uuid,
    pub reporter_user_id: Uuid,
    pub target_type: String,
    pub target_id: Uuid,
    pub reason: String,
    pub description: String,
    pub status: String,
    pub reviewed_by_user_id: Option<Uuid>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = reports)]
pub struct NewReportRow {
    pub id: Uuid,
    pub reporter_user_id: Uuid,
    pub target_type: String,
    pub target_id: Uuid,
    pub reason: String,
    pub description: String,
    pub status: String,
    pub reviewed_by_user_id: Option<Uuid>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable, Associations)]
#[diesel(table_name = index_jobs)]
#[diesel(belongs_to(UserRow, foreign_key = requested_by_user_id))]
pub struct IndexJobRow {
    pub id: Uuid,
    pub status: String,
    pub job_type: String,
    pub git_url: String,
    pub git_ref: String,
    pub git_subdir: String,
    pub repo_slug: Option<String>,
    pub requested_by_user_id: Uuid,
    pub result_data: Value,
    pub error_message: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub progress_pct: i32,
    pub progress_message: String,
    pub commit_sha: Option<String>,
    pub force_index: bool,
    pub url_hash: Option<String>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = index_jobs)]
pub struct NewIndexJobRow {
    pub id: Uuid,
    pub status: String,
    pub job_type: String,
    pub git_url: String,
    pub git_ref: String,
    pub git_subdir: String,
    pub repo_slug: Option<String>,
    pub requested_by_user_id: Uuid,
    pub result_data: Value,
    pub error_message: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub progress_pct: i32,
    pub progress_message: String,
    pub commit_sha: Option<String>,
    pub force_index: bool,
    pub url_hash: Option<String>,
}

#[derive(Debug, Default, Clone, AsChangeset)]
#[diesel(table_name = index_jobs)]
pub struct IndexJobChangeset {
    pub status: Option<String>,
    pub result_data: Option<Value>,
    pub error_message: Option<Option<String>>,
    pub started_at: Option<Option<DateTime<Utc>>>,
    pub completed_at: Option<Option<DateTime<Utc>>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub progress_pct: Option<i32>,
    pub progress_message: Option<String>,
    pub commit_sha: Option<Option<String>>,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable)]
#[diesel(table_name = security_scans)]
pub struct SecurityScanRow {
    pub id: Uuid,
    pub target_type: String,
    pub target_id: Uuid,
    pub scan_module: String,
    pub result: String,
    pub severity: Option<String>,
    pub details: Value,
    pub scanned_by_user_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub version_id: Option<Uuid>,
    pub commit_sha: Option<String>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = security_scans)]
pub struct NewSecurityScanRow {
    pub id: Uuid,
    pub target_type: String,
    pub target_id: Uuid,
    pub scan_module: String,
    pub result: String,
    pub severity: Option<String>,
    pub details: Value,
    pub scanned_by_user_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub version_id: Option<Uuid>,
    pub commit_sha: Option<String>,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable)]
#[diesel(table_name = ai_usage_logs)]
pub struct AiUsageLogRow {
    pub id: Uuid,
    pub task_type: String,
    pub provider: String,
    pub model: String,
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub total_tokens: i32,
    pub target_type: Option<String>,
    pub target_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = ai_usage_logs)]
pub struct NewAiUsageLogRow {
    pub id: Uuid,
    pub task_type: String,
    pub provider: String,
    pub model: String,
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub total_tokens: i32,
    pub target_type: Option<String>,
    pub target_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = ai_request_cache)]
pub struct NewAiRequestCacheRow {
    pub id: Uuid,
    pub task_type: String,
    pub target_type: String,
    pub target_id: Uuid,
    pub commit_sha: String,
    pub success: bool,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable)]
#[diesel(table_name = site_admins)]
pub struct SiteAdminRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub granted_by_user_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = site_admins)]
pub struct NewSiteAdminRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub granted_by_user_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Queryable, Selectable, Identifiable)]
#[diesel(table_name = index_rules)]
pub struct IndexRuleRow {
    pub id: Uuid,
    pub repo_url: String,
    pub path_regex: String,
    pub strategy: String,
    pub description: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = index_rules)]
pub struct NewIndexRuleRow {
    pub id: Uuid,
    pub repo_url: String,
    pub path_regex: String,
    pub strategy: String,
    pub description: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Default, Clone, AsChangeset)]
#[diesel(table_name = index_rules)]
pub struct IndexRuleChangeset {
    pub repo_url: Option<String>,
    pub path_regex: Option<String>,
    pub strategy: Option<String>,
    pub description: Option<String>,
    pub updated_at: Option<DateTime<Utc>>,
}
