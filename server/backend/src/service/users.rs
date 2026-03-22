use std::collections::{HashMap, HashSet};

use diesel::prelude::*;
use shared::{UserListItem, UserListResponse, UserProfileResponse, UserRole};
use uuid::Uuid;

use super::browse_history;
use super::helpers::{
    db_conn, load_skill_versions_map, load_users_map, skill_item_from_rows, user_summary_from_row,
};
use super::repos::{flock_summary_from_row, load_flock_skill_counts};
use crate::auth::RequestUser;
use crate::error::AppError;
use crate::models::{FlockRow, RepoRow, SkillRow, SkillStarRow, SkillVersionRow, UserRow};
use crate::schema::{flocks, repos, skill_stars, skill_versions, skills, users};

const PROFILE_SKILL_LIMIT: usize = 24;
const PROFILE_FLOCK_LIMIT: usize = 12;
const PROFILE_HISTORY_LIMIT: i64 = 20;

pub fn list_users(query: Option<&str>, limit: i64) -> Result<UserListResponse, AppError> {
    let mut conn = db_conn()?;
    let limit = limit.clamp(1, 100) as usize;
    let users_rows = users::table
        .order(users::handle.asc())
        .select(UserRow::as_select())
        .load::<UserRow>(&mut conn)?;
    let filtered = users_rows
        .into_iter()
        .filter(|row| match query {
            Some(query) if !query.trim().is_empty() => {
                let query = query.trim().to_lowercase();
                row.handle.to_lowercase().contains(&query)
                    || row
                        .display_name
                        .as_deref()
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains(&query)
            }
            _ => true,
        })
        .collect::<Vec<_>>();

    // Count skills per user by looking at flocks imported by each user
    let flock_rows = flocks::table
        .select(FlockRow::as_select())
        .load::<FlockRow>(&mut conn)?;
    let mut flocks_by_importer: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for flock in &flock_rows {
        flocks_by_importer
            .entry(flock.imported_by_user_id)
            .or_default()
            .push(flock.id);
    }
    let skill_rows = skills::table
        .filter(skills::soft_deleted_at.is_null())
        .select(SkillRow::as_select())
        .load::<SkillRow>(&mut conn)?;
    let mut skill_counts_by_flock: HashMap<Uuid, i64> = HashMap::new();
    for row in &skill_rows {
        *skill_counts_by_flock.entry(row.flock_id).or_default() += 1;
    }
    let mut skill_counts: HashMap<Uuid, i64> = HashMap::new();
    for (importer_id, flock_ids) in &flocks_by_importer {
        let count: i64 = flock_ids
            .iter()
            .map(|fid| skill_counts_by_flock.get(fid).copied().unwrap_or(0))
            .sum();
        if count > 0 {
            skill_counts.insert(*importer_id, count);
        }
    }

    let total = filtered.len() as i64;
    let items = filtered
        .into_iter()
        .take(limit)
        .map(|row| UserListItem {
            user: user_summary_from_row(&row),
            skill_count: *skill_counts.get(&row.id).unwrap_or(&0),
        })
        .collect();

    Ok(UserListResponse { items, total })
}

pub fn get_user_profile(
    handle_value: &str,
    viewer: Option<&RequestUser>,
) -> Result<UserProfileResponse, AppError> {
    let mut conn = db_conn()?;
    let user = users::table
        .filter(users::handle.eq(handle_value))
        .select(UserRow::as_select())
        .first::<UserRow>(&mut conn)
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("user `{handle_value}` not found")))?;

    let viewer_is_staff = viewer
        .map(|viewer| matches!(viewer.role, UserRole::Admin | UserRole::Moderator))
        .unwrap_or(false);
    let is_self = viewer.map(|viewer| viewer.id == user.id).unwrap_or(false);

    let published_skills = load_published_skills(&mut conn, user.id, viewer_is_staff)?;
    let starred_skills = if is_self {
        load_starred_skills(&mut conn, user.id, viewer_is_staff)?
    } else {
        Vec::new()
    };
    let starred_flocks = if is_self {
        load_starred_flocks(&mut conn, user.id)?
    } else {
        Vec::new()
    };
    let history = if is_self {
        browse_history::get_history_for_user_id_with_conn(
            &mut conn,
            user.id,
            PROFILE_HISTORY_LIMIT,
        )?
        .items
    } else {
        Vec::new()
    };

    Ok(UserProfileResponse {
        user: user_summary_from_row(&user),
        bio: user.bio.clone(),
        joined_at: user.created_at,
        github_login: user.github_login.clone(),
        is_self,
        published_skills,
        starred_skills,
        starred_flocks,
        history,
    })
}

fn load_published_skills(
    conn: &mut PgConnection,
    user_id: Uuid,
    viewer_is_staff: bool,
) -> Result<Vec<shared::SkillListItem>, AppError> {
    let version_rows = skill_versions::table
        .filter(skill_versions::created_by.eq(user_id))
        .filter(skill_versions::soft_deleted_at.is_null())
        .order(skill_versions::created_at.desc())
        .select(SkillVersionRow::as_select())
        .load::<SkillVersionRow>(conn)?;
    let skill_ids = ordered_unique_ids(
        version_rows.iter().filter_map(|row| row.skill_id),
        PROFILE_SKILL_LIMIT,
    );
    load_skill_items(conn, skill_ids, viewer_is_staff)
}

fn load_starred_skills(
    conn: &mut PgConnection,
    user_id: Uuid,
    viewer_is_staff: bool,
) -> Result<Vec<shared::SkillListItem>, AppError> {
    let star_rows = skill_stars::table
        .filter(skill_stars::user_id.eq(user_id))
        .filter(skill_stars::skill_id.is_not_null())
        .order(skill_stars::created_at.desc())
        .select(SkillStarRow::as_select())
        .load::<SkillStarRow>(conn)?;
    let skill_ids = ordered_unique_ids(
        star_rows.iter().filter_map(|row| row.skill_id),
        PROFILE_SKILL_LIMIT,
    );
    load_skill_items(conn, skill_ids, viewer_is_staff)
}

fn load_starred_flocks(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> Result<Vec<shared::FlockSummary>, AppError> {
    let star_rows = skill_stars::table
        .filter(skill_stars::user_id.eq(user_id))
        .filter(skill_stars::skill_id.is_null())
        .order(skill_stars::created_at.desc())
        .select(SkillStarRow::as_select())
        .load::<SkillStarRow>(conn)?;
    let flock_ids = ordered_unique_ids(
        star_rows.iter().map(|row| row.flock_id),
        PROFILE_FLOCK_LIMIT,
    );
    if flock_ids.is_empty() {
        return Ok(Vec::new());
    }

    let flock_rows = flocks::table
        .filter(flocks::id.eq_any(&flock_ids))
        .select(FlockRow::as_select())
        .load::<FlockRow>(conn)?;
    let repo_rows = repos::table
        .filter(repos::id.eq_any(flock_rows.iter().map(|row| row.repo_id).collect::<Vec<_>>()))
        .select(RepoRow::as_select())
        .load::<RepoRow>(conn)?;
    let importing_users = load_users_map(
        conn,
        flock_rows
            .iter()
            .map(|row| row.imported_by_user_id)
            .collect::<Vec<_>>(),
    )?;
    let skill_counts = load_flock_skill_counts(conn, flock_ids.clone())?;

    let flock_map = flock_rows
        .into_iter()
        .map(|row| (row.id, row))
        .collect::<HashMap<_, _>>();
    let repo_map = repo_rows
        .into_iter()
        .map(|row| (row.id, row))
        .collect::<HashMap<_, _>>();

    flock_ids
        .into_iter()
        .filter_map(|flock_id| {
            flock_map
                .get(&flock_id)
                .and_then(|flock| repo_map.get(&flock.repo_id).map(|repo| (flock, repo)))
        })
        .map(|(flock, repo)| {
            let skill_count = *skill_counts.get(&flock.id).unwrap_or(&0);
            flock_summary_from_row(flock, repo, &importing_users, skill_count)
        })
        .collect()
}

fn load_skill_items(
    conn: &mut PgConnection,
    ordered_skill_ids: Vec<Uuid>,
    viewer_is_staff: bool,
) -> Result<Vec<shared::SkillListItem>, AppError> {
    if ordered_skill_ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut query = skills::table
        .filter(skills::id.eq_any(&ordered_skill_ids))
        .filter(skills::soft_deleted_at.is_null())
        .into_boxed();
    if !viewer_is_staff {
        query = query.filter(skills::moderation_status.eq("active"));
    }
    let skill_rows = query.select(SkillRow::as_select()).load::<SkillRow>(conn)?;
    // Load owners via flocks (imported_by_user_id)
    let flock_ids: Vec<Uuid> = skill_rows
        .iter()
        .map(|row| row.flock_id)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let flock_rows = flocks::table
        .filter(flocks::id.eq_any(&flock_ids))
        .select(FlockRow::as_select())
        .load::<FlockRow>(conn)?;
    let owner_ids: Vec<Uuid> = flock_rows.iter().map(|f| f.imported_by_user_id).collect();
    let owners = load_users_map(conn, owner_ids)?;
    let flock_owner_map: HashMap<Uuid, Uuid> = flock_rows
        .iter()
        .map(|f| (f.id, f.imported_by_user_id))
        .collect();

    let latest_versions = load_skill_versions_map(
        conn,
        skill_rows
            .iter()
            .filter_map(|row| row.latest_version_id)
            .collect::<Vec<_>>(),
    )?;
    // Load repos for git_url (needed to derive sign)
    let repo_ids: Vec<Uuid> = skill_rows
        .iter()
        .map(|row| row.repo_id)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let repo_rows = repos::table
        .filter(repos::id.eq_any(&repo_ids))
        .select(RepoRow::as_select())
        .load::<RepoRow>(conn)?;
    let repo_map: HashMap<Uuid, RepoRow> = repo_rows.into_iter().map(|r| (r.id, r)).collect();

    let skill_map = skill_rows
        .into_iter()
        .map(|row| (row.id, row))
        .collect::<HashMap<_, _>>();

    ordered_skill_ids
        .into_iter()
        .filter_map(|skill_id| skill_map.get(&skill_id))
        .map(|row| {
            let owner_user_id = flock_owner_map
                .get(&row.flock_id)
                .copied()
                .unwrap_or(Uuid::nil());
            let owner = owners
                .get(&owner_user_id)
                .ok_or_else(|| AppError::Internal("missing skill owner".to_string()))?;
            let latest = row
                .latest_version_id
                .and_then(|id| latest_versions.get(&id));
            let git_url = repo_map
                .get(&row.repo_id)
                .map(|r| r.git_url.as_str())
                .unwrap_or_default();
            Ok(skill_item_from_rows(row, git_url, owner, latest))
        })
        .collect()
}

fn ordered_unique_ids<I>(ids: I, limit: usize) -> Vec<Uuid>
where
    I: IntoIterator<Item = Uuid>,
{
    let mut seen = HashSet::new();
    let mut ordered = Vec::new();
    for id in ids {
        if seen.insert(id) {
            ordered.push(id);
        }
        if ordered.len() >= limit {
            break;
        }
    }
    ordered
}
