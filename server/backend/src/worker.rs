use std::collections::HashSet;

use chrono::Utc;
use diesel::prelude::*;
use tokio::task::{JoinHandle, JoinSet};
use uuid::Uuid;

use crate::db::PgPool;
use crate::models::{IndexJobRow, NewIndexJobRow, RepoRow};
use crate::schema::{flocks, index_jobs, repos};
use crate::service::git_ops::resolve_remote_sha;
use crate::service::helpers::{hash_string, normalize_git_url};
use crate::state::app_state;

pub fn spawn_worker(pool: PgPool) -> JoinHandle<()> {
    tokio::spawn(async move {
        tracing::info!("background worker started");

        {
            match pool.get() {
                Ok(mut conn) => {
                    let recovered =
                        diesel::update(index_jobs::table.filter(index_jobs::status.eq("running")))
                            .set(crate::models::IndexJobChangeset {
                                status: Some("pending".to_string()),
                                started_at: Some(None),
                                updated_at: Some(Utc::now()),
                                progress_pct: Some(0),
                                progress_message: Some(
                                    "Queued (recovered after restart)".to_string(),
                                ),
                                ..Default::default()
                            })
                            .execute(&mut conn)
                            .unwrap_or(0);

                    if recovered > 0 {
                        tracing::info!("recovered {recovered} stale running job(s) -> pending");
                    }
                }
                Err(e) => {
                    tracing::error!("failed to get DB connection for job recovery: {e}");
                }
            }
        }

        let config = &app_state().config;
        let index_interval = std::time::Duration::from_secs(10);
        let repo_check_interval = std::time::Duration::from_secs(config.sync_interval_secs);

        let mut index_tick = tokio::time::interval(index_interval);
        let mut repo_check_tick = tokio::time::interval(repo_check_interval);
        let mut cleanup_tick =
            tokio::time::interval(std::time::Duration::from_secs(24 * 60 * 60));

        let mut index_tasks: JoinSet<(Uuid, String)> = JoinSet::new();
        let mut running_url_hashes: HashSet<String> = HashSet::new();

        loop {
            tokio::select! {
                _ = index_tick.tick() => {
                    while let Some(result) = index_tasks.try_join_next() {
                        match result {
                            Ok((job_id, url_hash)) => {
                                running_url_hashes.remove(&url_hash);
                                tracing::debug!(job_id = %job_id, "index task finished");
                            }
                            Err(e) => {
                                tracing::warn!("index task panicked: {e}");
                            }
                        }
                    }

                    let max_jobs = config.max_parallel_index_jobs;
                    if let Err(error) = dispatch_pending_index_jobs(
                        &pool,
                        &mut index_tasks,
                        &mut running_url_hashes,
                        max_jobs,
                    ).await {
                        tracing::warn!("index dispatch error: {error}");
                    }
                }
                _ = repo_check_tick.tick() => {
                    let pool = pool.clone();
                    tokio::spawn(async move {
                        if let Err(error) = check_repos_for_new_commits(&pool).await {
                            tracing::warn!("repo check error: {error}");
                        }
                    });
                }
                _ = cleanup_tick.tick() => {
                    match pool.get() {
                        Ok(mut conn) => {
                            match crate::service::browse_history::cleanup_old_history(&mut conn) {
                                Ok(n) if n > 0 => tracing::info!("cleaned up {n} old browse history entries"),
                                Ok(_) => {}
                                Err(e) => tracing::warn!("browse history cleanup error: {e}"),
                            }
                        }
                        Err(e) => tracing::warn!("browse history cleanup pool error: {e}"),
                    }
                }
                Some(result) = index_tasks.join_next(), if !index_tasks.is_empty() => {
                    match result {
                        Ok((job_id, url_hash)) => {
                            running_url_hashes.remove(&url_hash);
                            tracing::debug!(job_id = %job_id, "index task finished");
                        }
                        Err(e) => {
                            tracing::warn!("index task panicked: {e}");
                        }
                    }
                }
            }
        }
    })
}

async fn dispatch_pending_index_jobs(
    pool: &PgPool,
    tasks: &mut JoinSet<(Uuid, String)>,
    running_url_hashes: &mut HashSet<String>,
    max_jobs: usize,
) -> Result<(), String> {
    let available_slots = max_jobs.saturating_sub(tasks.len());
    if available_slots == 0 {
        return Ok(());
    }

    let pending_jobs = {
        let mut conn = pool.get().map_err(|e| e.to_string())?;
        index_jobs::table
            .filter(index_jobs::status.eq("pending"))
            .order(index_jobs::created_at.asc())
            .limit((available_slots * 3).max(20) as i64)
            .select(IndexJobRow::as_select())
            .load::<IndexJobRow>(&mut conn)
            .map_err(|e| e.to_string())?
    };

    let mut dispatched = 0usize;
    for job in pending_jobs {
        if dispatched >= available_slots {
            break;
        }

        let url_hash = job
            .url_hash
            .clone()
            .unwrap_or_else(|| hash_string(&normalize_git_url(&job.git_url)));

        if running_url_hashes.contains(&url_hash) {
            continue;
        }

        let job_id = job.id;
        let job_type = job.job_type.clone();
        let uh = url_hash.clone();
        running_url_hashes.insert(url_hash);

        tracing::info!(
            job_id = %job_id,
            job_type = %job_type,
            running = tasks.len() + 1,
            "dispatching index job"
        );

        let pool = pool.clone();
        tasks.spawn(async move {
            execute_index_job(&pool, job_id, &job_type).await;
            (job_id, uh)
        });
        dispatched += 1;
    }

    Ok(())
}

async fn execute_index_job(pool: &PgPool, job_id: Uuid, job_type: &str) {
    match job_type {
        "auto_import" => {
            if let Err(error) = crate::service::index_jobs::execute_auto_import(job_id).await {
                tracing::error!(job_id = %job_id, "auto_import failed: {error}");
            }
        }
        "resync" => {
            let result = pool.get().map_err(|e| e.to_string()).and_then(|mut conn| {
                diesel::update(index_jobs::table.find(job_id))
                    .set(crate::models::IndexJobChangeset {
                        status: Some("completed".to_string()),
                        completed_at: Some(Some(Utc::now())),
                        updated_at: Some(Utc::now()),
                        ..Default::default()
                    })
                    .execute(&mut conn)
                    .map_err(|e| e.to_string())
            });
            if let Err(e) = result {
                tracing::error!(job_id = %job_id, "resync status update failed: {e}");
            }
        }
        other => {
            tracing::warn!(job_id = %job_id, "unknown job type: {other}");
            let result = pool.get().map_err(|e| e.to_string()).and_then(|mut conn| {
                diesel::update(index_jobs::table.find(job_id))
                    .set(crate::models::IndexJobChangeset {
                        status: Some("failed".to_string()),
                        error_message: Some(Some(format!("unknown job type: {other}"))),
                        completed_at: Some(Some(Utc::now())),
                        updated_at: Some(Utc::now()),
                        ..Default::default()
                    })
                    .execute(&mut conn)
                    .map_err(|e| e.to_string())
            });
            if let Err(e) = result {
                tracing::error!(job_id = %job_id, "failed status update failed: {e}");
            }
        }
    }
}

async fn check_repos_for_new_commits(pool: &PgPool) -> Result<(), String> {
    let config = &app_state().config;
    let interval_secs = config.auto_index_min_interval_secs as i64;
    let threshold = Utc::now() - chrono::Duration::seconds(interval_secs);

    let repo = {
        let mut conn = pool.get().map_err(|e| e.to_string())?;
        repos::table
            .filter(
                repos::last_indexed_at
                    .is_null()
                    .or(repos::last_indexed_at.lt(threshold)),
            )
            .order(repos::last_indexed_at.asc().nulls_first())
            .select(RepoRow::as_select())
            .first::<RepoRow>(&mut conn)
            .optional()
            .map_err(|e| e.to_string())?
    };

    let Some(repo) = repo else {
        return Ok(());
    };

    let git_url = normalize_git_url(&repo.git_url);
    let url_hash = hash_string(&git_url);

    let current_sha = match resolve_remote_sha(&git_url, "HEAD").await {
        Ok(sha) => sha,
        Err(e) => {
            tracing::debug!(repo_id = %repo.id, "failed to ls-remote for auto-index: {e}");
            return Ok(());
        }
    };

    let mut conn = pool.get().map_err(|e| e.to_string())?;

    let already_indexed: i64 = index_jobs::table
        .filter(index_jobs::git_url.eq(&git_url))
        .filter(index_jobs::commit_sha.eq(&current_sha))
        .filter(index_jobs::status.eq("completed"))
        .count()
        .get_result(&mut conn)
        .map_err(|e: diesel::result::Error| e.to_string())?;

    if already_indexed > 0 {
        diesel::update(repos::table.find(repo.id))
            .set(crate::models::RepoChangeset {
                last_indexed_at: Some(Some(Utc::now())),
                updated_at: Some(Utc::now()),
                git_rev: Some(current_sha),
                ..Default::default()
            })
            .execute(&mut conn)
            .map_err(|e: diesel::result::Error| e.to_string())?;
        return Ok(());
    }

    let active_count: i64 = index_jobs::table
        .filter(index_jobs::url_hash.eq(&url_hash))
        .filter(index_jobs::status.eq_any(["pending", "running"]))
        .count()
        .get_result(&mut conn)
        .map_err(|e: diesel::result::Error| e.to_string())?;

    if active_count > 0 {
        return Ok(());
    }

    let now = Utc::now();
    let job_id = Uuid::now_v7();

    tracing::info!(
        repo_id = %repo.id,
        git_url = %git_url,
        commit_sha = %current_sha,
        "auto-creating index job for repo with new commits"
    );

    diesel::insert_into(index_jobs::table)
        .values(NewIndexJobRow {
            id: job_id,
            status: "pending".to_string(),
            job_type: "auto_import".to_string(),
            git_url,
            git_ref: "HEAD".to_string(),
            git_subdir: ".".to_string(),
            repo_slug: None,
            requested_by_user_id: {
                let mut flock_conn = pool.get().map_err(|e| e.to_string())?;
                flocks::table
                    .filter(flocks::repo_id.eq(repo.id))
                    .select(flocks::imported_by_user_id)
                    .first::<Uuid>(&mut flock_conn)
                    .unwrap_or(Uuid::nil())
            },
            result_data: serde_json::json!({}),
            error_message: None,
            started_at: None,
            completed_at: None,
            created_at: now,
            updated_at: now,
            progress_pct: 0,
            progress_message: "Queued (auto-index)".to_string(),
            commit_sha: Some(current_sha),
            force_index: false,
            url_hash: Some(url_hash),
        })
        .execute(&mut conn)
        .map_err(|e| e.to_string())?;

    Ok(())
}
