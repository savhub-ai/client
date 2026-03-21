//! One-time startup upgrade: backfill `repos.git_rev` for rows that are NULL.
//!
//! Strategy (per repo):
//! 1. Look up the newest completed `index_jobs.commit_sha` matching git_url.
//! 2. If still nothing, resolve the latest SHA from the remote via `git ls-remote`.

use chrono::Utc;
use diesel::prelude::*;

use crate::db::PgPool;
use crate::error::AppError;
use crate::models::{RepoChangeset, RepoRow};
use crate::schema::{index_jobs, repos};
use crate::service::helpers::normalize_git_url;
use crate::service::git_ops::resolve_remote_sha;

/// Backfill `git_rev` for every repo that currently has it set to NULL.
///
/// This is safe to call on every startup — it only touches rows that need it
/// and logs what it does.
pub async fn backfill_repo_git_rev(pool: &PgPool) -> Result<(), AppError> {
    let missing: Vec<RepoRow> = {
        let mut conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
        repos::table
            .filter(repos::git_rev.is_null())
            .select(RepoRow::as_select())
            .load::<RepoRow>(&mut conn)?
    };

    if missing.is_empty() {
        return Ok(());
    }

    tracing::info!(
        "[upgrade] {} repos with NULL git_rev — backfilling",
        missing.len()
    );

    for repo in &missing {
        let resolved = try_resolve_git_rev(pool, repo).await;
        match resolved {
            Some(sha) => {
                let mut conn = pool.get().map_err(|e| AppError::Internal(e.to_string()))?;
                diesel::update(repos::table.find(repo.id))
                    .set(RepoChangeset {
                        git_rev: Some(Some(sha.clone())),
                        updated_at: Some(Utc::now()),
                        ..Default::default()
                    })
                    .execute(&mut conn)?;
                tracing::info!(
                    "[upgrade] repo {} git_rev = {}",
                    repo.sign,
                    &sha[..sha.len().min(12)]
                );
            }
            None => {
                tracing::warn!(
                    "[upgrade] repo {} — could not resolve git_rev, skipping",
                    repo.sign
                );
            }
        }
    }

    tracing::info!("[upgrade] backfill_repo_git_rev finished");
    Ok(())
}

/// Try every data source in priority order; return the first SHA found.
async fn try_resolve_git_rev(pool: &PgPool, repo: &RepoRow) -> Option<String> {
    // 1) Newest completed index_job commit_sha matching git_url
    if let Some(sha) = from_index_jobs(pool, repo) {
        return Some(sha);
    }

    // 2) Live resolve from remote
    from_remote(repo).await
}

fn from_index_jobs(pool: &PgPool, repo: &RepoRow) -> Option<String> {
    let mut conn = pool.get().ok()?;
    let git_url = normalize_git_url(&repo.git_url);
    index_jobs::table
        .filter(index_jobs::git_url.eq(&git_url))
        .filter(index_jobs::status.eq("completed"))
        .filter(index_jobs::commit_sha.is_not_null())
        .order(index_jobs::completed_at.desc())
        .select(index_jobs::commit_sha)
        .first::<Option<String>>(&mut conn)
        .ok()
        .flatten()
}

async fn from_remote(repo: &RepoRow) -> Option<String> {
    let git_url = normalize_git_url(&repo.git_url);
    match resolve_remote_sha(&git_url, "HEAD").await {
        Ok(sha) => Some(sha),
        Err(e) => {
            tracing::debug!("[upgrade] ls-remote failed for {}: {e}", repo.sign);
            None
        }
    }
}
