use std::collections::{HashMap, HashSet};
use std::fs;

use chrono::Utc;
use diesel::prelude::*;
use serde_json::json;
use shared::{
    CatalogSource, FlockMetadata, ImportedSkillRecord, IndexJobDto, IndexJobListResponse,
    IndexJobStatus, RegistryStatus, SubmitIndexRequest, SubmitIndexResponse,
};
use uuid::Uuid;

use super::git_ops::{
    SkillCandidate, apply_repo_redirect, collect_skill_candidates, parse_skill_markdown_metadata,
    refresh_cached_repo, resolve_remote_sha, sanitize_skill_slug,
};
use super::helpers::{
    db_conn, hash_string, insert_audit_log, normalize_git_url, parse_git_url_parts,
};
use super::security::{ScanContext, run_automated_scans_with_files};
use crate::auth::AuthContext;
use crate::error::AppError;
use crate::models::{
    FlockChangeset, FlockRow, IndexJobChangeset, IndexJobRow, NewAiRequestCacheRow, NewFlockRow,
    NewIndexJobRow, NewRepoRow, NewSkillRow, RepoRow, SkillRow,
};
use crate::schema::{ai_request_cache, flocks, index_jobs, repos, skills};
use crate::state::app_state;

/// Check if we already have a **successful** AI request cached for this
/// (task_type, target_id, commit_sha) combination. Failed requests are
/// ignored so they get retried on the next index.
fn has_cached_ai_request(
    conn: &mut PgConnection,
    task_type: &str,
    target_id: Uuid,
    commit_sha: &str,
) -> bool {
    ai_request_cache::table
        .filter(ai_request_cache::task_type.eq(task_type))
        .filter(ai_request_cache::target_id.eq(target_id))
        .filter(ai_request_cache::commit_sha.eq(commit_sha))
        .filter(ai_request_cache::success.eq(true))
        .count()
        .get_result::<i64>(conn)
        .unwrap_or(0)
        > 0
}

/// Record an AI request result. On conflict (same task+target+commit),
/// update success/error so a retry after a failure overwrites the old record.
fn cache_ai_request(
    conn: &mut PgConnection,
    task_type: &str,
    target_type: &str,
    target_id: Uuid,
    commit_sha: &str,
    success: bool,
    error_message: Option<&str>,
) {
    let _ = diesel::insert_into(ai_request_cache::table)
        .values(NewAiRequestCacheRow {
            id: Uuid::now_v7(),
            task_type: task_type.to_string(),
            target_type: target_type.to_string(),
            target_id,
            commit_sha: commit_sha.to_string(),
            success,
            error_message: error_message.map(|s| s.to_string()),
            created_at: Utc::now(),
        })
        .on_conflict((
            ai_request_cache::task_type,
            ai_request_cache::target_id,
            ai_request_cache::commit_sha,
        ))
        .do_update()
        .set((
            ai_request_cache::success.eq(success),
            ai_request_cache::error_message.eq(error_message),
            ai_request_cache::created_at.eq(Utc::now()),
        ))
        .execute(conn);
}

/// Check whether the given user is a site admin.
fn is_site_admin(conn: &mut PgConnection, user_id: Uuid) -> bool {
    use crate::schema::site_admins;
    site_admins::table
        .filter(site_admins::user_id.eq(user_id))
        .count()
        .get_result::<i64>(conn)
        .unwrap_or(0)
        > 0
}

/// Find an existing completed scan job with the same git_url and commit_sha.
fn find_existing_index(
    conn: &mut PgConnection,
    git_url: &str,
    git_ref: &str,
    git_subdir: &str,
    repo_slug: Option<&str>,
    commit_sha: &str,
) -> Result<Option<IndexJobRow>, AppError> {
    let mut query = index_jobs::table
        .filter(index_jobs::git_url.eq(git_url))
        .filter(index_jobs::git_ref.eq(git_ref))
        .filter(index_jobs::git_subdir.eq(git_subdir))
        .filter(index_jobs::commit_sha.eq(commit_sha))
        .filter(index_jobs::status.eq("completed"))
        .order(index_jobs::created_at.desc())
        .into_boxed();

    query = match repo_slug {
        Some(repo_slug) => query.filter(index_jobs::repo_slug.eq(Some(repo_slug.to_string()))),
        None => query.filter(index_jobs::repo_slug.is_null()),
    };

    let row = query
        .select(IndexJobRow::as_select())
        .first::<IndexJobRow>(&mut *conn)
        .optional()?;
    Ok(row)
}

/// Find an active (pending/running) scan job with the given url_hash.
fn find_active_index_by_url_hash(
    conn: &mut PgConnection,
    url_hash: &str,
) -> Result<Option<IndexJobRow>, AppError> {
    let row = index_jobs::table
        .filter(index_jobs::url_hash.eq(url_hash))
        .filter(index_jobs::status.eq_any(["pending", "running"]))
        .order(index_jobs::created_at.desc())
        .select(IndexJobRow::as_select())
        .first::<IndexJobRow>(conn)
        .optional()?;
    Ok(row)
}

/// Mark an active job as superseded and notify WS subscribers.
pub(crate) fn supersede_index_job(conn: &mut PgConnection, job_id: Uuid) -> Result<(), AppError> {
    diesel::update(index_jobs::table.find(job_id))
        .set(IndexJobChangeset {
            status: Some("superseded".to_string()),
            completed_at: Some(Some(Utc::now())),
            updated_at: Some(Utc::now()),
            progress_message: Some("Superseded by newer scan".to_string()),
            ..Default::default()
        })
        .execute(conn)?;
    let _ = app_state()
        .events_tx
        .send(crate::state::WsEvent::IndexProgress {
            job_id,
            status: "superseded".to_string(),
            progress_pct: 100,
            progress_message: "Superseded by newer scan".to_string(),
            result_data: json!({}),
            error_message: None,
        });
    Ok(())
}

pub async fn submit_index(
    auth: &AuthContext,
    request: SubmitIndexRequest,
) -> Result<SubmitIndexResponse, AppError> {
    if request.git_url.trim().is_empty() {
        return Err(AppError::BadRequest("git_url is required".to_string()));
    }

    // Normalize the git URL so the same repo always produces the same data
    let git_url = normalize_git_url(&request.git_url);
    let url_hash = hash_string(&git_url);

    let mut conn = db_conn()?;

    // Only admins may set force=true
    let is_admin =
        matches!(auth.user.role, shared::UserRole::Admin) || is_site_admin(&mut conn, auth.user.id);
    let force = request.force && is_admin;
    println!(
        "[index] user={} role={:?} request.force={} is_site_admin={} → force={}",
        auth.user.handle, auth.user.role, request.force, is_admin, force
    );

    // Resolve the remote ref to a commit SHA before creating the job
    let commit_sha = resolve_remote_sha(&git_url, &request.git_ref).await?;

    // Check for an existing completed scan with the same url + sha
    if !force {
        if let Some(existing) = find_existing_index(
            &mut conn,
            &git_url,
            &request.git_ref,
            &request.git_subdir,
            request.repo_slug.as_deref(),
            &commit_sha,
        )? {
            println!(
                "[index] skipping duplicate scan for {} @ {} (existing job {})",
                git_url, commit_sha, existing.id
            );
            return Ok(SubmitIndexResponse {
                ok: true,
                job_id: existing.id,
                skipped: true,
                existing_job_id: Some(existing.id),
            });
        }
    }

    // Smart dedup: check for active (pending/running) job with same url_hash
    if !force {
        if let Some(active) = find_active_index_by_url_hash(&mut conn, &url_hash)? {
            if active.commit_sha.as_deref() == Some(&commit_sha) {
                // Same commit — return existing job
                println!(
                    "[index] reusing active job {} for {} @ {}",
                    active.id, git_url, commit_sha
                );
                return Ok(SubmitIndexResponse {
                    ok: true,
                    job_id: active.id,
                    skipped: true,
                    existing_job_id: Some(active.id),
                });
            } else {
                // Different commit — supersede old job, create new one
                println!(
                    "[index] superseding active job {} for {} (old={} new={})",
                    active.id,
                    git_url,
                    active.commit_sha.as_deref().unwrap_or("?"),
                    commit_sha
                );
                supersede_index_job(&mut conn, active.id)?;
            }
        }
    }

    let now = Utc::now();
    let job_id = Uuid::now_v7();

    diesel::insert_into(index_jobs::table)
        .values(NewIndexJobRow {
            id: job_id,
            status: "pending".to_string(),
            job_type: "auto_import".to_string(),
            git_url,
            git_ref: request.git_ref,
            git_subdir: request.git_subdir,
            repo_slug: request.repo_slug,
            requested_by_user_id: auth.user.id,
            result_data: json!({}),
            error_message: None,
            started_at: None,
            completed_at: None,
            created_at: now,
            updated_at: now,
            progress_pct: 0,
            progress_message: "Queued".to_string(),
            commit_sha: Some(commit_sha),
            force_index: force,
            url_hash: Some(url_hash),
        })
        .execute(&mut conn)?;

    Ok(SubmitIndexResponse {
        ok: true,
        job_id,
        skipped: false,
        existing_job_id: None,
    })
}

pub fn get_index_job(auth: &AuthContext, job_id: Uuid) -> Result<IndexJobDto, AppError> {
    let mut conn = db_conn()?;
    let row = index_jobs::table
        .find(job_id)
        .select(IndexJobRow::as_select())
        .first::<IndexJobRow>(&mut conn)
        .optional()?
        .ok_or_else(|| AppError::NotFound("scan job not found".to_string()))?;

    // Only owner or staff can view
    let is_staff = matches!(
        auth.user.role,
        shared::UserRole::Admin | shared::UserRole::Moderator
    );
    if row.requested_by_user_id != auth.user.id && !is_staff {
        return Err(AppError::Forbidden(
            "you do not have access to this scan job".to_string(),
        ));
    }

    Ok(index_job_dto_from_row(&row))
}

pub fn list_index_jobs(
    auth: &AuthContext,
    limit: i64,
    offset: i64,
) -> Result<IndexJobListResponse, AppError> {
    let mut conn = db_conn()?;
    let capped_limit = limit.min(100);
    let rows = index_jobs::table
        .filter(index_jobs::requested_by_user_id.eq(auth.user.id))
        .order(index_jobs::created_at.desc())
        .offset(offset.max(0))
        .limit(capped_limit + 1) // fetch one extra to detect has_more
        .select(IndexJobRow::as_select())
        .load::<IndexJobRow>(&mut conn)?;

    let has_more = rows.len() as i64 > capped_limit;
    let jobs: Vec<_> = rows
        .iter()
        .take(capped_limit as usize)
        .map(index_job_dto_from_row)
        .collect();

    Ok(IndexJobListResponse { jobs, has_more })
}

pub async fn execute_auto_import(job_id: Uuid) -> Result<(), AppError> {
    let pool = &app_state().pool;
    let config = &app_state().config;

    // Mark running
    {
        let mut conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
        diesel::update(index_jobs::table.find(job_id))
            .set(IndexJobChangeset {
                status: Some("running".to_string()),
                started_at: Some(Some(Utc::now())),
                updated_at: Some(Utc::now()),
                progress_pct: Some(0),
                progress_message: Some("Starting…".to_string()),
                ..Default::default()
            })
            .execute(&mut conn)?;
        let _ = app_state()
            .events_tx
            .send(crate::state::WsEvent::IndexProgress {
                job_id,
                status: "running".to_string(),
                progress_pct: 0,
                progress_message: "Starting…".to_string(),
                result_data: serde_json::json!({}),
                error_message: None,
            });
    }

    // Load the job
    let job = {
        let mut conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
        index_jobs::table
            .find(job_id)
            .select(IndexJobRow::as_select())
            .first::<IndexJobRow>(&mut conn)?
    };

    // ── Freshness check ──
    // Before doing real work, verify the remote repo hasn't advanced past the
    // commit this job was created for.  This is especially important for jobs
    // recovered after a server restart: the repo may have received new pushes
    // while the server was down.
    if let Some(ref job_sha) = job.commit_sha {
        match resolve_remote_sha(&job.git_url, &job.git_ref).await {
            Ok(current_sha) if current_sha != *job_sha => {
                tracing::info!(
                    job_id = %job_id,
                    old_sha = %job_sha,
                    new_sha = %current_sha,
                    "repo has newer commits — superseding and re-queuing"
                );
                let mut conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
                supersede_index_job(&mut conn, job_id)?;
                // Create a replacement job targeting the latest commit
                let now = Utc::now();
                let new_job_id = Uuid::now_v7();
                diesel::insert_into(index_jobs::table)
                    .values(NewIndexJobRow {
                        id: new_job_id,
                        status: "pending".to_string(),
                        job_type: job.job_type.clone(),
                        git_url: job.git_url.clone(),
                        git_ref: job.git_ref.clone(),
                        git_subdir: job.git_subdir.clone(),
                        repo_slug: job.repo_slug.clone(),
                        requested_by_user_id: job.requested_by_user_id,
                        result_data: json!({}),
                        error_message: None,
                        started_at: None,
                        completed_at: None,
                        created_at: now,
                        updated_at: now,
                        progress_pct: 0,
                        progress_message: "Queued (superseded stale job)".to_string(),
                        commit_sha: Some(current_sha),
                        force_index: job.force_index,
                        url_hash: job.url_hash.clone(),
                    })
                    .execute(&mut conn)?;
                tracing::info!(
                    old_job_id = %job_id,
                    new_job_id = %new_job_id,
                    "replacement job created — will be picked up on next tick"
                );
                return Ok(());
            }
            Ok(_) => {
                tracing::debug!(job_id = %job_id, "remote SHA unchanged, proceeding");
            }
            Err(e) => {
                // Network error — proceed with the existing job rather than failing
                tracing::warn!(
                    job_id = %job_id,
                    "could not resolve remote SHA for freshness check: {e} — proceeding anyway"
                );
            }
        }
    }

    match do_auto_import(&job, config).await {
        Ok(result_data) => {
            let mut conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
            diesel::update(index_jobs::table.find(job_id))
                .set(IndexJobChangeset {
                    status: Some("completed".to_string()),
                    result_data: Some(result_data.clone()),
                    completed_at: Some(Some(Utc::now())),
                    updated_at: Some(Utc::now()),
                    progress_pct: Some(100),
                    progress_message: Some("Done".to_string()),
                    ..Default::default()
                })
                .execute(&mut conn)?;
            let _ = app_state()
                .events_tx
                .send(crate::state::WsEvent::IndexProgress {
                    job_id,
                    status: "completed".to_string(),
                    progress_pct: 100,
                    progress_message: "Done".to_string(),
                    result_data,
                    error_message: None,
                });
        }
        Err(error) => {
            let err_msg = error.to_string();
            let mut conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
            diesel::update(index_jobs::table.find(job_id))
                .set(IndexJobChangeset {
                    status: Some("failed".to_string()),
                    error_message: Some(Some(err_msg.clone())),
                    completed_at: Some(Some(Utc::now())),
                    updated_at: Some(Utc::now()),
                    progress_pct: Some(100),
                    progress_message: Some(format!("Failed: {}", err_msg)),
                    ..Default::default()
                })
                .execute(&mut conn)?;
            let _ = app_state()
                .events_tx
                .send(crate::state::WsEvent::IndexProgress {
                    job_id,
                    status: "failed".to_string(),
                    progress_pct: 100,
                    progress_message: format!("Failed: {}", err_msg),
                    result_data: serde_json::json!({}),
                    error_message: Some(err_msg),
                });
        }
    }

    Ok(())
}

async fn do_auto_import(
    job: &IndexJobRow,
    config: &crate::config::Config,
) -> Result<serde_json::Value, AppError> {
    update_progress(job.id, 10, "Preparing repo…")?;
    let checkout = refresh_cached_repo(
        &config.repo_checkout_base_path(),
        &job.git_url,
        &job.git_ref,
    )
    .await?;
    println!(
        "[index{}] repo ready {} (ref={}) -> {} reused={} commit={}",
        job.id,
        job.git_url,
        job.git_ref,
        checkout.path.display(),
        checkout.reused,
        checkout.head_sha
    );
    // If git followed a redirect, update the stored URL before proceeding.
    // Use the new URL for all subsequent repo_sign / domain / path_slug derivations.
    let effective_git_url = if let Some(ref new_url) = checkout.redirected_url {
        tracing::warn!(
            job_id = %job.id,
            old_url = job.git_url.as_str(),
            new_url = new_url.as_str(),
            "repo has been moved/renamed — applying redirect"
        );
        let normalized = normalize_git_url(new_url);
        // If the repo already exists under the old URL, migrate it in-place
        let mut redir_conn = db_conn()?;
        let old_git_url = normalize_git_url(&job.git_url);
        if let Some(existing) = repos::table
            .filter(repos::git_url.eq(&old_git_url))
            .select(RepoRow::as_select())
            .first::<RepoRow>(&mut redir_conn)
            .optional()?
        {
            apply_repo_redirect(&mut redir_conn, existing.id, &job.git_url, new_url)?;
        }
        normalized
    } else {
        job.git_url.clone()
    };

    // Resolve index rule (strategy + scan path) before determining scan root.
    update_progress(job.id, 20, "Resolving index rules…")?;
    let repo_name = extract_repo_name(&effective_git_url);
    let (domain, path_slug) = parse_git_url_parts(&effective_git_url);
    let repo_sign = format!("{}/{}", domain, path_slug);
    println!(
        "[index{}] repo_name={}, repo_sign={}",
        job.id, repo_name, repo_sign
    );

    let resolved = {
        let mut rule_conn = app_state()
            .pool
            .get()
            .map_err(|e| AppError::Internal(e.to_string()))?;
        super::index_rules::resolve_index_rule(&mut rule_conn, &job.git_url, &job.git_subdir)?
    };
    println!(
        "[index{}] resolved rule: strategy={:?}, scan_path={}",
        job.id, resolved.strategy, resolved.scan_path
    );

    // Determine scan root from the resolved scan_path
    let effective_subdir = &resolved.scan_path;
    let scan_root = if effective_subdir == "." {
        checkout.path.clone()
    } else {
        checkout.path.join(effective_subdir)
    };
    println!("[index{}] scan_root={}", job.id, scan_root.display());

    if !scan_root.exists() {
        println!("[index{}] scan_root does not exist, aborting", job.id);
        return Err(AppError::BadRequest(format!(
            "subdir `{}` not found in repo",
            effective_subdir
        )));
    }

    // Collect skill candidates
    update_progress(job.id, 30, "Scanning for skills…")?;
    let mut candidates = Vec::new();
    collect_skill_candidates(&scan_root, ".", &mut candidates)?;
    println!(
        "[index{}] found {} SKILL.md candidates",
        job.id,
        candidates.len()
    );

    for (i, c) in candidates.iter().enumerate() {
        println!(
            "[index{}]   candidate[{}]: relative_dir={:?}, path={}",
            job.id,
            i,
            c.relative_dir,
            c.path.display()
        );
    }

    if candidates.is_empty() {
        return Err(AppError::BadRequest(
            "no SKILL.md files found in repo".to_string(),
        ));
    }

    // Auto-categorize using the resolved strategy.
    update_progress(job.id, 50, "Categorizing skills…")?;

    let groups = match resolved.strategy {
        super::index_rules::IndexStrategy::EachDirAsFlock => {
            compute_each_dir_as_flock_plans(&candidates, &repo_name)
        }
        super::index_rules::IndexStrategy::Smart => {
            compute_flock_group_plans(&candidates, &repo_name)
        }
    };
    println!(
        "[index{}] grouping: {:?} → {} flock group(s)",
        job.id,
        resolved.strategy,
        groups.len()
    );
    for group in &groups {
        let source_path = join_repo_relative_path(effective_subdir, &group.source_path);
        println!(
            "[index{}]   group: slug={:?}, source_path={:?}, candidates={}",
            job.id,
            group.slug,
            source_path,
            group.candidate_indices.len()
        );
    }

    let repo_description = extract_repo_description(&checkout.path, &repo_name);
    let repo_description_for_check = repo_description.clone();

    // Ensure repo exists; update name and description on reindex.
    // Acquire a short-lived connection for the repo setup, then release it
    // so the pool is not held during the (potentially long) per-flock loop.
    let repo = {
        let mut conn = db_conn()?;
        let repo = if let Some(existing) = repos::table
            .filter(repos::git_url.eq(&effective_git_url))
            .select(RepoRow::as_select())
            .first::<RepoRow>(&mut conn)
            .optional()?
        {
            diesel::update(repos::table.find(existing.id))
                .set(crate::models::RepoChangeset {
                    name: Some(repo_name.to_string()),
                    updated_at: Some(Utc::now()),
                    ..Default::default()
                })
                .execute(&mut conn)?;
            RepoRow {
                name: repo_name.to_string(),
                ..existing
            }
        } else {
            let now = Utc::now();
            let row = NewRepoRow {
                id: Uuid::now_v7(),
                name: repo_name.to_string(),
                description: repo_description,
                git_url: effective_git_url.clone(),
                license: None,
                visibility: "public".to_string(),
                verified: false,
                metadata: json!({}),
                keywords: vec![],
                created_at: now,
                updated_at: now,
                last_indexed_at: None,
                git_hash: checkout.head_sha.clone(),
                git_branch: None,
            };
            diesel::insert_into(repos::table)
                .values(&row)
                .execute(&mut conn)?;

            repos::table
                .filter(repos::git_url.eq(&effective_git_url))
                .select(RepoRow::as_select())
                .first::<RepoRow>(&mut conn)?
        };

        // Clean up previous index data: delete all skills and flocks for this repo
        // so re-indexing starts fresh
        {
            let old_flocks = flocks::table
                .filter(flocks::repo_id.eq(repo.id))
                .select(FlockRow::as_select())
                .load::<FlockRow>(&mut conn)?;
            for old_flock in &old_flocks {
                diesel::delete(skills::table.filter(skills::flock_id.eq(old_flock.id)))
                    .execute(&mut conn)?;
            }
            diesel::delete(flocks::table.filter(flocks::repo_id.eq(repo.id))).execute(&mut conn)?;
            if !old_flocks.is_empty() {
                println!(
                    "[index{}] cleaned up {} previous flock(s) for re-indexing",
                    job.id,
                    old_flocks.len()
                );
            }
        }

        repo
    }; // conn is dropped here — returned to the pool

    let mut flock_slugs = Vec::new();
    let mut total_skill_count = 0usize;
    let total_groups = groups.len();

    println!(
        "[index{}] {} flock group(s) to persist",
        job.id, total_groups
    );
    for (group_idx, group) in groups.iter().enumerate() {
        if group.slug.is_empty() {
            continue;
        }

        let flock_slug = group.slug.clone();
        let skill_count = group.candidate_indices.len();

        // Check ai_request_cache to see if flock metadata was already generated
        // for this commit — if so, reuse existing flock row data and skip AI.
        let existing_flock_for_meta = {
            let mut conn = db_conn()?;
            flocks::table
                .filter(flocks::repo_id.eq(repo.id))
                .filter(flocks::slug.eq(&flock_slug))
                .select(FlockRow::as_select())
                .first::<FlockRow>(&mut conn)
                .optional()?
        };
        let flock_meta_cached = existing_flock_for_meta.as_ref().map_or(false, |f| {
            let mut conn = db_conn().ok();
            conn.as_mut()
                .map(|c| has_cached_ai_request(c, "flock_metadata", f.id, &checkout.head_sha))
                .unwrap_or(false)
        });

        let default_flock_name = derive_flock_name(&flock_slug, &repo_name);
        let (flock_name, flock_description) = if flock_meta_cached {
            let f = existing_flock_for_meta.as_ref().unwrap();
            tracing::info!(
                "[index] ai_request_cache hit for flock_metadata {}/{} (commit {})",
                repo_sign,
                flock_slug,
                &checkout.head_sha[..checkout.head_sha.len().min(8)],
            );
            (f.name.clone(), f.description.clone())
        } else if skill_count == 1 {
            let c = &candidates[group.candidate_indices[0]];
            if let Ok(md) = fs::read_to_string(c.path.join("SKILL.md")) {
                if let Ok(meta) = parse_skill_markdown_metadata(&c.relative_dir, &flock_slug, &md) {
                    (meta.name, meta.description)
                } else {
                    (
                        default_flock_name,
                        format!("Auto-imported flock: {flock_slug}"),
                    )
                }
            } else {
                (
                    default_flock_name,
                    format!("Auto-imported flock: {flock_slug}"),
                )
            }
        } else {
            let skill_summaries: Vec<(String, String)> = group
                .candidate_indices
                .iter()
                .filter_map(|&i| {
                    let c = &candidates[i];
                    let md = fs::read_to_string(c.path.join("SKILL.md")).ok()?;
                    let meta =
                        parse_skill_markdown_metadata(&c.relative_dir, &flock_slug, &md).ok()?;
                    Some((meta.name, meta.description))
                })
                .collect();
            let readme_content = ["README.md", "readme.md", "Readme.md", "README"]
                .iter()
                .find_map(|f| fs::read_to_string(checkout.path.join(f)).ok());
            if let Some(ai_meta) =
                super::ai::generate_flock_metadata(&skill_summaries, readme_content.as_deref())
                    .await
            {
                (ai_meta.name, ai_meta.description)
            } else {
                (
                    default_flock_name,
                    format!("Auto-imported flock: {flock_slug}"),
                )
            }
        };

        // Progress: 70..95 spread across flocks
        let pct = 70 + (group_idx * 25) / total_groups.max(1);
        update_progress(
            job.id,
            pct as i32,
            &format!(
                "Persisting flock {}/{} ({}/{})",
                repo_sign,
                flock_slug,
                group_idx + 1,
                total_groups
            ),
        )?;

        let skills = build_auto_import_skills(
            &candidates,
            &group.candidate_indices,
            &flock_slug,
            &job.git_url,
            &job.git_ref,
            effective_subdir,
            &repo_sign,
        )?;

        if skills.is_empty() {
            continue;
        }

        // Build file contents for security scanning
        let scan_files: Vec<super::security::SkillScanInput> = skills
            .iter()
            .map(|skill_rec| {
                let skill_dir = group
                    .candidate_indices
                    .iter()
                    .find_map(|&i| {
                        let c = &candidates[i];
                        if c.relative_dir.ends_with(&skill_rec.path)
                            || skill_rec.slug == sanitize_skill_slug(&c.relative_dir)
                        {
                            Some(c.path.clone())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| checkout.path.join(&skill_rec.path));

                let files = walkdir::WalkDir::new(&skill_dir)
                    .max_depth(3)
                    .into_iter()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().is_file())
                    .filter_map(|e| {
                        let rel = e
                            .path()
                            .strip_prefix(&skill_dir)
                            .ok()?
                            .to_string_lossy()
                            .to_string();
                        let content = fs::read_to_string(e.path()).ok()?;
                        Some(super::security_scan::FileContent { path: rel, content })
                    })
                    .collect();

                super::security::SkillScanInput {
                    skill_id: skill_rec.id.unwrap_or_default(),
                    slug: skill_rec.slug.clone(),
                    name: skill_rec.name.clone(),
                    description: skill_rec.description.clone(),
                    license: skill_rec.license.clone(),
                    version: skill_rec.version.clone(),
                    metadata_json: None,
                    frontmatter_always: None,
                    files,
                    version_id: None,
                }
            })
            .collect();

        let source_path = join_repo_relative_path(effective_subdir, &group.source_path);

        // Acquire a fresh connection per flock to avoid holding a connection
        // across the entire (potentially long) loop.
        {
            let mut conn = db_conn()?;
            persist_auto_import_flock(
                &mut conn,
                &repo,
                &flock_slug,
                &flock_name,
                &flock_description,
                &source_path,
                &checkout.head_sha,
                job.requested_by_user_id,
                &skills,
                "scan_job.auto_import",
                &scan_files,
            )?;
        }

        // Record successful flock metadata in ai_request_cache so future
        // re-indexes with the same commit can skip the AI call.
        if !flock_meta_cached {
            let mut conn = db_conn()?;
            if let Ok(flock_row) = flocks::table
                .filter(flocks::repo_id.eq(repo.id))
                .filter(flocks::slug.eq(&flock_slug))
                .select(FlockRow::as_select())
                .first::<FlockRow>(&mut conn)
            {
                cache_ai_request(
                    &mut conn,
                    "flock_metadata",
                    "flock",
                    flock_row.id,
                    &checkout.head_sha,
                    true,
                    None,
                );
            }
        }

        flock_slugs.push(flock_slug);
        total_skill_count += skill_count;
    }

    // Enhance repo metadata after all flocks are persisted.
    //
    // Check ai_request_cache for repo metadata.
    let repo_meta_cached = {
        let mut conn = db_conn()?;
        has_cached_ai_request(&mut conn, "repo_metadata", repo.id, &checkout.head_sha)
    };

    let is_fallback_desc = repo_description_for_check.starts_with("Auto-created repo for ");
    let mut repo_update = crate::models::RepoChangeset {
        updated_at: Some(Utc::now()),
        last_indexed_at: Some(Some(Utc::now())),
        git_hash: Some(checkout.head_sha.clone()),
        git_branch: Some(Some(job.git_ref.clone())),
        ..Default::default()
    };
    if repo_meta_cached {
        tracing::info!(
            "[index] ai_request_cache hit for repo_metadata {} (commit {})",
            repo_sign,
            &checkout.head_sha[..checkout.head_sha.len().min(8)],
        );
    } else if flock_slugs.len() == 1 {
        let mut conn = db_conn()?;
        if let Ok(flock_row) = flocks::table
            .filter(flocks::repo_id.eq(repo.id))
            .select(crate::models::FlockRow::as_select())
            .first::<crate::models::FlockRow>(&mut conn)
        {
            tracing::info!(
                "[index] single flock — updating repo {} with flock name={:?} desc={:?}",
                repo_sign,
                flock_row.name,
                &flock_row.description[..flock_row.description.len().min(60)],
            );
            repo_update.name = Some(flock_row.name.clone());
            repo_update.description = Some(flock_row.description.clone());
            repo_update.keywords = Some(flock_row.keywords.clone());
        }
        cache_ai_request(
            &mut conn,
            "repo_metadata",
            "repo",
            repo.id,
            &checkout.head_sha,
            true,
            None,
        );
    } else if is_fallback_desc {
        // Try AI generation from README
        let readme_content = ["README.md", "readme.md", "Readme.md", "README"]
            .iter()
            .find_map(|f| fs::read_to_string(checkout.path.join(f)).ok());
        if let Some(content) = readme_content {
            match super::ai::generate_repo_metadata(&content).await {
                Some(ai_meta) => {
                    repo_update.name = Some(ai_meta.name);
                    repo_update.description = Some(ai_meta.description);
                    repo_update.keywords = Some(ai_meta.keywords.into_iter().map(Some).collect());
                    let mut conn = db_conn()?;
                    cache_ai_request(
                        &mut conn,
                        "repo_metadata",
                        "repo",
                        repo.id,
                        &checkout.head_sha,
                        true,
                        None,
                    );
                }
                None => {
                    let mut conn = db_conn()?;
                    cache_ai_request(
                        &mut conn,
                        "repo_metadata",
                        "repo",
                        repo.id,
                        &checkout.head_sha,
                        false,
                        Some("AI returned no result"),
                    );
                }
            }
        }
    }
    {
        let mut conn = db_conn()?;
        diesel::update(repos::table.find(repo.id))
            .set(&repo_update)
            .execute(&mut conn)?;
    }

    update_progress(job.id, 95, "Finalizing…")?;
    println!(
        "[index{}] done: {} flocks, {} skills total. repo cached at {}",
        job.id,
        flock_slugs.len(),
        total_skill_count,
        checkout.path.display()
    );

    Ok(json!({
        "repo": repo_sign,
        "flocks": flock_slugs,
        "skill_count": total_skill_count,
        "commit_sha": checkout.head_sha,
        "previous_commit_sha": checkout.previous_sha,
        "changed_skill_files": checkout.changed_skill_files,
        "repo_path": checkout.path.display().to_string(),
        "reused_checkout": checkout.reused,
    }))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FlockGroupPlan {
    slug: String,
    source_path: String,
    candidate_indices: Vec<usize>,
}

pub(crate) fn build_auto_import_skills(
    candidates: &[SkillCandidate],
    indices: &[usize],
    flock_slug: &str,
    _git_url: &str,
    _git_ref: &str,
    _source_path_root: &str,
    repo_sign: &str,
) -> Result<Vec<ImportedSkillRecord>, AppError> {
    let mut skills = Vec::new();
    let mut seen_slugs = HashSet::new();

    for &i in indices {
        let candidate = &candidates[i];
        let skill_path = join_repo_relative_path(_source_path_root, &candidate.relative_dir);
        let markdown = match fs::read_to_string(candidate.path.join("SKILL.md")) {
            Ok(content) => content,
            Err(error) => {
                tracing::warn!("Skipping SKILL.md in {}: {error}", candidate.relative_dir);
                continue;
            }
        };
        let metadata = match parse_skill_markdown_metadata(&skill_path, flock_slug, &markdown) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };

        // Build the skill sign to derive context for the display name
        let skill_sign = format!("{repo_sign}/{skill_path}");
        let formatted_name = format_skill_name(&metadata.name, &skill_sign);
        let slug = format_skill_slug(&formatted_name);

        if slug.is_empty() || !seen_slugs.insert(slug.clone()) {
            continue;
        }

        skills.push(ImportedSkillRecord {
            id: None,
            slug,
            path: skill_path,
            name: formatted_name,
            description: Some(metadata.description),
            version: metadata.version.clone(),
            status: RegistryStatus::Active,
            license: "MIT".to_string(),
            runtime: None,
            security: shared::SecuritySummary::default(),
            metadata: shared::ImportedSkillMetadata::default(),
        });
    }

    skills.sort_by(|left, right| left.slug.cmp(&right.slug));
    Ok(skills)
}

pub(crate) fn persist_auto_import_flock(
    conn: &mut PgConnection,
    repo: &RepoRow,
    flock_slug: &str,
    flock_name: &str,
    flock_description: &str,
    source_path: &str,
    commit_sha: &str,
    user_id: Uuid,
    skills: &[ImportedSkillRecord],
    audit_action: &str,
    skill_scan_files: &[super::security::SkillScanInput],
) -> Result<(), AppError> {
    let now = Utc::now();
    let existing_flock = flocks::table
        .filter(flocks::repo_id.eq(repo.id))
        .filter(flocks::slug.eq(flock_slug))
        .select(FlockRow::as_select())
        .first::<FlockRow>(conn)
        .optional()?;

    let flock_id = existing_flock
        .as_ref()
        .map(|r| r.id)
        .unwrap_or_else(Uuid::new_v4);

    let flock_source = serde_json::to_value(CatalogSource::Registry {
        path: source_path.to_string(),
    })
    .unwrap_or_default();

    let flock_metadata = serde_json::to_value(FlockMetadata::default()).unwrap_or_default();

    // Check ai_request_cache for a previous successful security scan of
    // this flock at the same commit. If found, skip re-scanning entirely.
    let prev_security_status = existing_flock.as_ref().map(|f| f.security_status.as_str());
    let already_scanned = has_cached_ai_request(conn, "security_scan", flock_id, commit_sha);
    if already_scanned {
        tracing::info!(
            "[security] ai_request_cache hit for security_scan flock {} (commit {}), status={:?}",
            flock_id,
            &commit_sha[..commit_sha.len().min(8)],
            prev_security_status,
        );
    }

    if let Some(existing) = &existing_flock {
        diesel::update(flocks::table.find(existing.id))
            .set(FlockChangeset {
                name: Some(flock_name.to_string()),
                description: Some(flock_description.to_string()),
                version: None,
                source: Some(flock_source.clone()),
                metadata: Some(flock_metadata),
                updated_at: Some(now),
                // Preserve security_status if already scanned.
                security_status: if already_scanned {
                    None // None = no change
                } else {
                    Some("unverified".to_string())
                },
                ..Default::default()
            })
            .execute(conn)?;
        diesel::delete(skills::table.filter(skills::flock_id.eq(existing.id))).execute(conn)?;
    } else {
        diesel::insert_into(flocks::table)
            .values(NewFlockRow {
                id: flock_id,
                repo_id: repo.id,
                slug: flock_slug.to_string(),
                name: flock_name.to_string(),
                keywords: vec![],
                description: flock_description.to_string(),
                version: None,
                status: "active".to_string(),
                visibility: Some("public".to_string()),
                license: None,
                metadata: flock_metadata,
                source: flock_source.clone(),
                imported_by_user_id: user_id,
                created_at: now,
                updated_at: now,
                stats_comments: 0,
                stats_ratings: 0,
                stats_avg_rating: 0.0,
                security_status: "unverified".to_string(),
                stats_max_installs: 0,
                stats_max_unique_users: 0,
            })
            .execute(conn)?;
    }

    // Insert skill entries
    let mut skill_rows = Vec::with_capacity(skills.len());
    for skill in skills {
        skill_rows.push(NewSkillRow {
            id: Uuid::now_v7(),
            slug: skill.slug.clone(),
            name: skill.name.clone(),
            path: skill.path.clone(),
            keywords: vec![],
            repo_id: repo.id,
            flock_id,
            description: skill.description.clone(),
            version: skill.version.clone(),
            status: "active".to_string(),
            license: Some(skill.license.clone()),
            source: flock_source.clone(),
            metadata: serde_json::to_value(&skill.metadata).unwrap_or_default(),
            entry_data: None,
            runtime_data: skill
                .runtime
                .as_ref()
                .map(|r| serde_json::to_value(r).unwrap_or_default()),
            security_status: "unverified".to_string(),
            latest_version_id: None,
            tags: serde_json::json!({}),
            moderation_status: "active".to_string(),
            highlighted: false,
            official: false,
            deprecated: false,
            suspicious: false,
            stats_downloads: 0,
            stats_stars: 0,
            stats_versions: 0,
            stats_comments: 0,
            stats_installs: 0,
            stats_unique_users: 0,
            soft_deleted_at: None,
            created_at: now,
            updated_at: now,
        });
    }

    // Diesel/PostgreSQL limit: max 65535 bind parameters per query.
    // NewSkillRow has 30 fields, so batch at most 2000 rows per INSERT.
    for chunk in skill_rows.chunks(500) {
        diesel::insert_into(skills::table)
            .values(chunk)
            .execute(conn)?;
    }

    // Run automated security scans — but skip if this flock already has a
    // definitive scan result from a previous index (verified/flagged/rejected).
    if !already_scanned {
        let entry_rows = skills::table
            .filter(skills::flock_id.eq(flock_id))
            .select(SkillRow::as_select())
            .load::<SkillRow>(conn)?;
        let scan_ctx = ScanContext {
            commit_sha: Some(commit_sha.to_string()),
        };
        run_automated_scans_with_files(
            conn,
            flock_id,
            &entry_rows,
            skill_scan_files,
            Some(&scan_ctx),
        )?;
        // Record successful scan so future re-indexes with same commit skip it.
        cache_ai_request(
            conn,
            "security_scan",
            "flock",
            flock_id,
            commit_sha,
            true,
            None,
        );
    } else {
        // Propagate the existing flock security_status to newly-inserted skills
        // so they inherit the previous scan result.
        let status = prev_security_status.unwrap_or("unverified");
        diesel::update(skills::table.filter(skills::flock_id.eq(flock_id)))
            .set(skills::security_status.eq(status))
            .execute(conn)?;
    }

    // Re-read flock security_status after scans and populate SecuritySummary
    // so the registry JSON includes scanned_commit for client verification.
    //
    // When scans were skipped (already_scanned), the scanned_commit must
    insert_audit_log(
        conn,
        Some(user_id),
        audit_action,
        "flock",
        Some(flock_id),
        json!({
            "repo_sign": &repo.git_url,
            "flock_slug": flock_slug,
            "skill_count": skills.len(),
            "source_path": source_path,
        }),
    )?;

    Ok(())
}

// ---------------------------------------------------------------------------
// each_dir_as_flock grouping: every matched directory becomes a flock
// ---------------------------------------------------------------------------

fn compute_each_dir_as_flock_plans(
    candidates: &[SkillCandidate],
    repo_name: &str,
) -> Vec<FlockGroupPlan> {
    // Each SKILL.md directory becomes its own flock.  The full relative
    // directory is used as the group key so that deeply-nested repos
    // (e.g. skills/<author>/<skill>/SKILL.md) produce one flock per leaf.
    let mut plans: Vec<FlockGroupPlan> = candidates
        .iter()
        .enumerate()
        .map(|(i, candidate)| {
            let dir = &candidate.relative_dir;
            let (slug, source_path) = if dir == "." {
                (sanitize_skill_slug(repo_name), ".".to_string())
            } else {
                (sanitize_skill_slug(dir), dir.clone())
            };
            FlockGroupPlan {
                slug,
                source_path,
                candidate_indices: vec![i],
            }
        })
        .collect();
    plans.sort_by(|a, b| a.slug.cmp(&b.slug));
    plans
}

// ---------------------------------------------------------------------------
// LCA-based skill grouping
// ---------------------------------------------------------------------------

fn path_segments(relative_dir: &str) -> Vec<&str> {
    if relative_dir == "." {
        vec![]
    } else {
        relative_dir.split('/').collect()
    }
}

fn join_repo_relative_path(base: &str, child: &str) -> String {
    match (base, child) {
        (".", ".") => ".".to_string(),
        (".", child) => child.to_string(),
        (base, ".") => base.to_string(),
        (base, child) => format!("{base}/{child}"),
    }
}

fn find_lca(candidates: &[SkillCandidate]) -> Vec<String> {
    let all_segs: Vec<Vec<&str>> = candidates
        .iter()
        .map(|c| path_segments(&c.relative_dir))
        .collect();

    if all_segs.is_empty() {
        return vec![];
    }

    let min_len = all_segs.iter().map(|s| s.len()).min().unwrap_or(0);
    let mut lca = Vec::new();

    for i in 0..min_len {
        let segment = all_segs[0][i];
        if all_segs.iter().all(|segs| segs[i] == segment) {
            lca.push(segment.to_string());
        } else {
            break;
        }
    }

    lca
}

fn group_source_path(
    lca: &[String],
    lca_len: usize,
    candidates: &[SkillCandidate],
    indices: &[usize],
) -> String {
    let Some(&index) = indices.first() else {
        return ".".to_string();
    };
    let segs = path_segments(&candidates[index].relative_dir);
    if segs.len() <= lca_len {
        return ".".to_string();
    }

    let mut parts = lca.to_vec();
    parts.push(segs[lca_len].to_string());
    if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    }
}

fn compute_flock_group_plans(
    candidates: &[SkillCandidate],
    repo_name: &str,
) -> Vec<FlockGroupPlan> {
    let lca = find_lca(candidates);
    let lca_len = lca.len();

    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();

    for (i, candidate) in candidates.iter().enumerate() {
        let segs = path_segments(&candidate.relative_dir);
        let key = if segs.len() > lca_len {
            sanitize_skill_slug(segs[lca_len])
        } else {
            sanitize_skill_slug(repo_name)
        };
        groups.entry(key).or_default().push(i);
    }

    let num_groups = groups.len();
    let max_size = groups
        .values()
        .map(|indices| indices.len())
        .max()
        .unwrap_or(0);

    if num_groups < 2 || max_size < 2 {
        return vec![FlockGroupPlan {
            slug: sanitize_skill_slug(repo_name),
            source_path: ".".to_string(),
            candidate_indices: (0..candidates.len()).collect(),
        }];
    }

    let mut plans = groups
        .into_iter()
        .map(|(slug, candidate_indices)| FlockGroupPlan {
            source_path: group_source_path(&lca, lca_len, candidates, &candidate_indices),
            slug,
            candidate_indices,
        })
        .collect::<Vec<_>>();
    plans.sort_by(|left, right| left.slug.cmp(&right.slug));
    plans
}

#[cfg(test)]
fn compute_flock_groups(
    candidates: &[SkillCandidate],
    repo_name: &str,
) -> HashMap<String, Vec<usize>> {
    compute_flock_group_plans(candidates, repo_name)
        .into_iter()
        .map(|plan| (plan.slug, plan.candidate_indices))
        .collect()
}

/// Dedup adjacent `/`-separated segments whose first word matches (case-insensitive),
/// replace `-` with space, join with space.
/// When the first word of the current segment equals the first word of the previous
/// segment, the previous segment is replaced by the current one.
/// e.g. `"anthropics/mcp-skills"` → `"anthropics mcp skills"`
/// e.g. `"mofa/mofa-skills"` → `"mofa skills"`
pub(crate) fn path_to_display_name(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let mut deduped: Vec<String> = Vec::new();
    for part in &parts {
        let expanded = part.replace('-', " ");
        let cur_first = expanded.split_whitespace().next().unwrap_or("");
        if let Some(prev) = deduped.last() {
            let prev_first = prev.split_whitespace().next().unwrap_or("");
            if prev_first.eq_ignore_ascii_case(cur_first) {
                // Replace the previous segment with the current (more specific) one
                *deduped.last_mut().unwrap() = expanded;
                continue;
            }
        }
        deduped.push(expanded);
    }
    deduped.join(" ")
}

/// Build a human-readable repo name from the git URL path (domain stripped).
fn extract_repo_name(git_url: &str) -> String {
    let (_domain, path_slug) = parse_git_url_parts(git_url);
    if path_slug.is_empty() {
        return "imported".to_string();
    }
    let name = path_to_display_name(&path_slug);
    if name.is_empty() {
        "imported".to_string()
    } else {
        name
    }
}

/// Extract repo description from the README title (first `#` heading) in the checkout.
/// Falls back to `"Auto-created repo for {repo_name}"`.
fn extract_repo_description(checkout_path: &std::path::Path, repo_name: &str) -> String {
    let default = format!("Auto-created repo for {repo_name}");
    for candidate in &["README.md", "readme.md", "Readme.md", "README"] {
        let path = checkout_path.join(candidate);
        if let Ok(content) = fs::read_to_string(&path) {
            if let Some(heading) = content.lines().find_map(|line| {
                let trimmed = line.trim();
                // Match `# Title` but not `##` or deeper
                if trimmed.starts_with("# ") {
                    let title = trimmed.trim_start_matches('#').trim();
                    if !title.is_empty() {
                        return Some(title.to_string());
                    }
                }
                None
            }) {
                return heading;
            }
        }
    }
    default
}

/// Derive a flock display name.  When the flock slug is just "skill" or "skills"
/// the flock inherits the repo name (optionally appended with the slug when the
/// repo name does not already end with skill/skills).
fn derive_flock_name(flock_slug: &str, repo_name: &str) -> String {
    let slug_lower = flock_slug.to_ascii_lowercase();
    if slug_lower == "skill" || slug_lower == "skills" {
        let repo_lower = repo_name.to_ascii_lowercase();
        if repo_lower.ends_with("skill") || repo_lower.ends_with("skills") {
            repo_name.to_string()
        } else {
            format!("{} {}", repo_name, flock_slug.replace('-', " "))
        }
    } else {
        flock_slug.replace('-', " ")
    }
}

const NOISE_WORDS: &[&str] = &["skill", "skills", "ai", "agent", "agents"];

/// Format a skill display name.
///
/// 1. Take the original name from SKILL.md.
/// 2. Remove noise words (skill, skills, ai, agent, agents). If the remaining name still has > 2
///    words, use it directly.
/// 3. Otherwise build a context prefix from the skill sign: strip the first segment (domain) and
///    last segment (skill dir), capitalize each word, dedup adjacent `/`-parts, then prepend to the
///    original name.
pub(crate) fn format_skill_name(original_name: &str, skill_sign: &str) -> String {
    // If the name has no spaces but contains hyphens, split by '-' and rejoin
    let name = if !original_name.contains(' ') && original_name.contains('-') {
        capitalize_words(&original_name.replace('-', " "))
    } else {
        original_name.to_string()
    };

    let filtered: Vec<&str> = name
        .split_whitespace()
        .filter(|w| !NOISE_WORDS.contains(&w.to_ascii_lowercase().as_str()))
        .collect();

    if filtered.len() > 2 {
        return name;
    }

    // Build context from sign: remove first (domain) and last (skill dir) fragments
    let sign_parts: Vec<&str> = skill_sign.split('/').collect();
    if sign_parts.len() > 2 {
        let middle = &sign_parts[1..sign_parts.len() - 1];
        let middle_path = middle.join("/");
        let context = path_to_display_name(&middle_path);
        let context = capitalize_words(&context);
        if !context.is_empty() {
            return format!("{} {}", context, name);
        }
    }

    name
}

/// Derive skill slug from the formatted skill name: lowercase, spaces → dashes.
pub(crate) fn format_skill_slug(formatted_name: &str) -> String {
    sanitize_skill_slug(formatted_name)
}

/// Words with special casing that should be preserved as-is.
const SPECIAL_CASE_WORDS: &[&str] = &[
    "iPhone",
    "iPad",
    "iPod",
    "iOS",
    "iMac",
    "iTunes",
    "iCloud",
    "macOS",
    "tvOS",
    "watchOS",
    "visionOS",
    "GitHub",
    "GitLab",
    "BitBucket",
    "DevOps",
    "DevTools",
    "JavaScript",
    "TypeScript",
    "GraphQL",
    "PostgreSQL",
    "MySQL",
    "SQLite",
    "MongoDB",
    "NoSQL",
    "OpenAI",
    "ChatGPT",
    "LangChain",
    "LlamaIndex",
    "FastAPI",
    "NextJS",
    "NodeJS",
    "NestJS",
    "VueJS",
    "ReactJS",
    "AngularJS",
    "OAuth",
    "WebSocket",
    "WebRTC",
    "gRPC",
    "VS",
    "VSCode",
    "YouTube",
    "LinkedIn",
    "TikTok",
    "WhatsApp",
    "WordPress",
    "MCP",
    "API",
    "APIs",
    "SDK",
    "CLI",
    "GUI",
    "URL",
    "URLs",
    "AI",
    "LLM",
    "LLMs",
    "NLP",
    "ML",
    "RAG",
    "AWS",
    "GCP",
    "CDN",
    "DNS",
    "SSH",
    "SSL",
    "TLS",
    "HTTP",
    "HTTPS",
    "JSON",
    "XML",
    "YAML",
    "TOML",
    "CSV",
    "HTML",
    "CSS",
    "CI",
    "CD",
    "QA",
    "ID",
    "UTF",
    "PDF",
    "SVG",
    "PNG",
    "JPG",
    "GIF",
    "eBay",
    "eBook",
];

fn capitalize_words(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            // Check if any special-case word matches (case-insensitive)
            if let Some(special) = SPECIAL_CASE_WORDS
                .iter()
                .find(|&&sc| sc.eq_ignore_ascii_case(w))
            {
                return special.to_string();
            }
            // Default: capitalize first letter
            let mut chars = w.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    format!("{}{}", upper, chars.as_str())
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn index_job_dto_from_row(row: &IndexJobRow) -> IndexJobDto {
    IndexJobDto {
        id: row.id,
        status: parse_index_job_status(&row.status),
        job_type: row.job_type.clone(),
        git_url: row.git_url.clone(),
        git_ref: row.git_ref.clone(),
        git_subdir: row.git_subdir.clone(),
        repo_slug: row.repo_slug.clone(),
        result_data: row.result_data.clone(),
        error_message: row.error_message.clone(),
        progress_pct: row.progress_pct,
        progress_message: row.progress_message.clone(),
        started_at: row.started_at,
        completed_at: row.completed_at,
        created_at: row.created_at,
    }
}

fn update_progress(job_id: Uuid, pct: i32, message: &str) -> Result<(), AppError> {
    let state = app_state();
    let mut conn = state
        .pool
        .get()
        .map_err(|e| AppError::Internal(e.to_string()))?;
    diesel::update(index_jobs::table.find(job_id))
        .set(IndexJobChangeset {
            progress_pct: Some(pct),
            progress_message: Some(message.to_string()),
            updated_at: Some(Utc::now()),
            ..Default::default()
        })
        .execute(&mut conn)?;
    let _ = state.events_tx.send(crate::state::WsEvent::IndexProgress {
        job_id,
        status: "running".to_string(),
        progress_pct: pct,
        progress_message: message.to_string(),
        result_data: serde_json::json!({}),
        error_message: None,
    });
    Ok(())
}

pub(crate) fn parse_index_job_status(value: &str) -> IndexJobStatus {
    match value {
        "running" => IndexJobStatus::Running,
        "completed" => IndexJobStatus::Completed,
        "failed" => IndexJobStatus::Failed,
        "superseded" => IndexJobStatus::Superseded,
        _ => IndexJobStatus::Pending,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn candidate(relative_dir: &str) -> SkillCandidate {
        SkillCandidate {
            path: PathBuf::from(relative_dir),
            relative_dir: relative_dir.to_string(),
        }
    }

    #[test]
    fn test_path_segments() {
        assert_eq!(path_segments("."), Vec::<&str>::new());
        assert_eq!(
            path_segments("skills/lang/python"),
            vec!["skills", "lang", "python"]
        );
        assert_eq!(path_segments("tools"), vec!["tools"]);
    }

    #[test]
    fn test_join_repo_relative_path() {
        assert_eq!(join_repo_relative_path(".", "."), ".");
        assert_eq!(
            join_repo_relative_path(".", "skills/python"),
            "skills/python"
        );
        assert_eq!(join_repo_relative_path("skills", "."), "skills");
        assert_eq!(join_repo_relative_path("skills", "python"), "skills/python");
    }

    #[test]
    fn test_find_lca_no_common() {
        let candidates = vec![candidate("coding/assistant"), candidate("devops/deployer")];
        assert_eq!(find_lca(&candidates), Vec::<String>::new());
    }

    #[test]
    fn test_find_lca_shared_prefix() {
        let candidates = vec![
            candidate("skills/lang/python"),
            candidate("skills/lang/rust"),
            candidate("skills/devops/deploy"),
        ];
        assert_eq!(find_lca(&candidates), vec!["skills"]);
    }

    #[test]
    fn test_find_lca_deep_shared_prefix() {
        let candidates = vec![
            candidate("src/skills/frontend/react"),
            candidate("src/skills/frontend/vue"),
            candidate("src/skills/backend/api"),
        ];
        assert_eq!(find_lca(&candidates), vec!["src", "skills"]);
    }

    #[test]
    fn test_find_lca_with_root_forces_empty() {
        // Root skill (segments=[]) forces min_len=0, so LCA is always empty
        let candidates = vec![
            candidate("."),
            candidate("skills/lang/python"),
            candidate("skills/lang/rust"),
        ];
        assert_eq!(find_lca(&candidates), Vec::<String>::new());
    }

    #[test]
    fn test_single_skill_at_root() {
        let candidates = vec![candidate(".")];
        let groups = compute_flock_groups(&candidates, "my-tool");
        assert_eq!(groups.len(), 1);
        assert!(groups.contains_key("my-tool"));
        assert_eq!(groups["my-tool"], vec![0]);
    }

    #[test]
    fn test_deep_nesting_with_categories() {
        // E1: skills/lang/python, skills/lang/rust, skills/devops/deploy
        let candidates = vec![
            candidate("skills/lang/python"),
            candidate("skills/lang/rust"),
            candidate("skills/devops/deploy"),
        ];
        let groups = compute_flock_groups(&candidates, "toolbox");

        // LCA = ["skills"], keys: lang(2), devops(1) → 2 groups, max=2 ✓
        assert_eq!(groups.len(), 2);
        assert_eq!(groups["lang"].len(), 2);
        assert_eq!(groups["devops"].len(), 1);
    }

    #[test]
    fn test_no_common_prefix_multi_flock() {
        // E2: coding/assistant, coding/reviewer, devops/deployer
        let candidates = vec![
            candidate("coding/assistant"),
            candidate("coding/reviewer"),
            candidate("devops/deployer"),
        ];
        let groups = compute_flock_groups(&candidates, "repo");

        // LCA = [], keys: coding(2), devops(1) → 2 groups, max=2 ✓
        assert_eq!(groups.len(), 2);
        assert_eq!(groups["coding"].len(), 2);
        assert_eq!(groups["devops"].len(), 1);
    }

    #[test]
    fn test_quality_check_fallback_all_singletons() {
        // E3: skills/python, skills/rust, skills/go — each group has size 1
        let candidates = vec![
            candidate("skills/python"),
            candidate("skills/rust"),
            candidate("skills/go"),
        ];
        let groups = compute_flock_groups(&candidates, "tools");

        // LCA = ["skills"], keys: python(1), rust(1), go(1) → max=1 ✗ → single flock
        assert_eq!(groups.len(), 1);
        assert!(groups.contains_key("tools"));
        assert_eq!(groups["tools"].len(), 3);
    }

    #[test]
    fn test_mixed_root_and_nested() {
        // E5: root + tools/debug, tools/lint
        let candidates = vec![
            candidate("."),
            candidate("tools/debug"),
            candidate("tools/lint"),
        ];
        let groups = compute_flock_groups(&candidates, "my-repo");

        // LCA = [] (root excluded), keys: my-repo(1), tools(2) → 2 groups, max=2 ✓
        assert_eq!(groups.len(), 2);
        assert_eq!(groups["my-repo"].len(), 1);
        assert_eq!(groups["tools"].len(), 2);
    }

    #[test]
    fn test_flat_siblings_single_each() {
        // E8: coding-assistant, code-reviewer, test-writer — each is its own group
        let candidates = vec![
            candidate("coding-assistant"),
            candidate("code-reviewer"),
            candidate("test-writer"),
        ];
        let groups = compute_flock_groups(&candidates, "skills");

        // LCA = [], keys: coding-assistant(1), code-reviewer(1), test-writer(1)
        // max=1 ✗ → single flock
        assert_eq!(groups.len(), 1);
        assert!(groups.contains_key("skills"));
        assert_eq!(groups["skills"].len(), 3);
    }

    #[test]
    fn test_deep_common_prefix_with_categories() {
        // E6: src/skills/frontend/react, src/skills/frontend/vue, src/skills/backend/api
        let candidates = vec![
            candidate("src/skills/frontend/react"),
            candidate("src/skills/frontend/vue"),
            candidate("src/skills/backend/api"),
        ];
        let groups = compute_flock_groups(&candidates, "repo");

        // LCA = ["src", "skills"], keys: frontend(2), backend(1) → 2 groups, max=2 ✓
        assert_eq!(groups.len(), 2);
        assert_eq!(groups["frontend"].len(), 2);
        assert_eq!(groups["backend"].len(), 1);
    }

    #[test]
    fn test_group_plans_capture_source_paths() {
        let candidates = vec![
            candidate("skills/lang/python"),
            candidate("skills/lang/rust"),
            candidate("skills/devops/deploy"),
        ];

        let plans = compute_flock_group_plans(&candidates, "toolbox");

        assert_eq!(plans.len(), 2);
        assert_eq!(plans[0].slug, "devops");
        assert_eq!(plans[0].source_path, "skills/devops");
        assert_eq!(plans[1].slug, "lang");
        assert_eq!(plans[1].source_path, "skills/lang");
    }

    #[test]
    fn test_group_plans_fallback_to_repo_root_path() {
        let candidates = vec![
            candidate("skills/python"),
            candidate("skills/rust"),
            candidate("skills/go"),
        ];

        let plans = compute_flock_group_plans(&candidates, "toolbox");

        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].slug, "toolbox");
        assert_eq!(plans[0].source_path, ".");
    }
}
