use chrono::Utc;
use diesel::dsl::count_star;
use diesel::prelude::*;
use serde_json::json;
use uuid::Uuid;

use crate::auth::{AuthContext, can_manage_owner};
use crate::error::AppError;
use crate::models::{FlockRow, NewSkillVersionRow, SkillChangeset};
use crate::schema::{flocks, skill_versions, skills};
use shared::{IndexRequest, PublishResponse, ResourceKind};

use super::helpers::{
    db_conn, fetch_skill_by_slug, insert_audit_log, parse_tag_map, prepare_index,
};

pub fn index_skill(auth: &AuthContext, request: IndexRequest) -> Result<PublishResponse, AppError> {
    let mut conn = db_conn()?;
    let index = prepare_index(&request, ResourceKind::Skill)?;
    let now = Utc::now();

    conn.transaction::<_, AppError, _>(|conn| {
        let existing_skill = fetch_skill_by_slug(conn, &index.slug)?;
        if let Some(existing_skill) = existing_skill.as_ref() {
            // Check ownership via flock importer
            let flock = flocks::table
                .find(existing_skill.flock_id)
                .select(FlockRow::as_select())
                .first::<FlockRow>(conn)?;
            if !can_manage_owner(auth, flock.imported_by_user_id) {
                return Err(AppError::Forbidden(
                    "you do not have permission to index a new version of this skill".to_string(),
                ));
            }
        }

        let skill_id = existing_skill
            .as_ref()
            .map(|row| row.id)
            .unwrap_or_else(Uuid::new_v4);

        let (repo_id, flock_id) = if let Some(ref existing) = existing_skill {
            (existing.repo_id, Some(existing.flock_id))
        } else {
            return Err(AppError::BadRequest(
                "cannot publish a new skill without a repo and flock context; import the skill through a flock first".to_string(),
            ));
        };

        let version_row = NewSkillVersionRow {
            id: Uuid::now_v7(),
            skill_id: Some(skill_id),
            repo_id,
            flock_id,
            git_sha: String::new(),
            git_ref: String::new(),
            version: Some(index.version.clone()),
            changelog: index.changelog.clone(),
            tags: index.tags.clone(),
            files: serde_json::to_value(&index.files)
                .map_err(|error| AppError::Internal(error.to_string()))?,
            parsed_metadata: index.parsed_metadata.clone(),
            search_document: index.search_document.clone(),
            fingerprint: index.fingerprint.clone(),
            scan_commit_hash: String::new(),
            created_by: auth.user.id,
            created_at: now,
            soft_deleted_at: None,
            scan_summary: None,
        };

        let version_exists = skill_versions::table
            .filter(skill_versions::skill_id.eq(skill_id))
            .filter(skill_versions::version.eq(&index.version))
            .select(count_star())
            .first::<i64>(conn)?;
        if version_exists > 0 {
            return Err(AppError::Conflict(format!(
                "version {} already exists for {}",
                index.version, index.slug
            )));
        }

        // existing_skill is always Some at this point (new skill path returns error above)
        if let Some(existing_skill) = existing_skill {
            let mut tags = parse_tag_map(&existing_skill.tags);
            for tag in &index.tags {
                tags.insert(tag.clone(), index.version.clone());
            }
            tags.insert("latest".to_string(), index.version.clone());
            diesel::update(skills::table.find(existing_skill.id))
                .set(SkillChangeset {
                    name: Some(index.display_name.clone()),
                    description: Some(index.summary.clone()),
                    latest_version_id: Some(version_row.id),
                    tags: Some(
                        serde_json::to_value(tags)
                            .map_err(|error| AppError::Internal(error.to_string()))?,
                    ),
                    stats_versions: Some(existing_skill.stats_versions + 1),
                    updated_at: Some(now),
                    ..Default::default()
                })
                .execute(conn)?;
        }

        diesel::insert_into(skill_versions::table)
            .values(version_row.clone())
            .execute(conn)?;

        insert_audit_log(
            conn,
            Some(auth.user.id),
            "skill.index",
            "skill",
            Some(skill_id),
            json!({
                "slug": index.slug,
                "version": index.version,
                "tags": index.tags,
            }),
        )?;

        Ok(PublishResponse {
            ok: true,
            resource_id: skill_id,
            version_id: version_row.id,
        })
    })
}
