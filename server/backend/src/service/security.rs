use chrono::Utc;
use diesel::prelude::*;
use serde_json::json;
use shared::{
    ScanVerdict, SecurityScanDto, SecurityScanListResponse, SecurityStatus,
    StaticScanFinding as SharedStaticScanFinding, StaticScanResult as SharedStaticScanResult,
    UpdateSecurityStatusRequest, VersionScanSummary,
};
use uuid::Uuid;

use super::helpers::{
    db_conn, fetch_flock_by_slugs, insert_audit_log, load_users_map, user_summary_from_row,
};
use super::llm_eval::{
    self, FileEntry, FileManifestEntry, SkillEvalContext, detect_injection_patterns,
};
use super::security_scan::{
    self, FileContent, ModerationVerdict, ScanInput, StaticScanResult, build_moderation_verdict,
};
use crate::auth::{AuthContext, require_staff};
use crate::error::AppError;
use crate::models::{FlockChangeset, NewSecurityScanRow, SecurityScanRow, SkillRow};
use crate::schema::{flocks, security_scans, skill_versions, skills};

// ---------------------------------------------------------------------------
// Staff endpoints (unchanged)
// ---------------------------------------------------------------------------

pub fn update_flock_security_status(
    auth: &AuthContext,
    repo_domain: &str,
    repo_path_slug: &str,
    flock_slug: &str,
    request: UpdateSecurityStatusRequest,
) -> Result<serde_json::Value, AppError> {
    require_staff(auth)?;
    let mut conn = db_conn()?;
    let repo_sign = format!("{repo_domain}/{repo_path_slug}");
    let flock = fetch_flock_by_slugs(&mut conn, &repo_sign, flock_slug)?;

    let status_str = security_status_to_str(request.security_status);

    diesel::update(flocks::table.find(flock.id))
        .set(FlockChangeset {
            security_status: Some(status_str.to_string()),
            updated_at: Some(Utc::now()),
            ..Default::default()
        })
        .execute(&mut conn)?;

    // Propagate to all skill entries
    diesel::update(skills::table.filter(skills::flock_id.eq(flock.id)))
        .set(skills::security_status.eq(status_str))
        .execute(&mut conn)?;

    insert_audit_log(
        &mut conn,
        Some(auth.user.id),
        "flock.security_status_update",
        "flock",
        Some(flock.id),
        json!({
            "repo_domain": repo_domain,
            "repo_path_slug": repo_path_slug,
            "flock_slug": flock_slug,
            "security_status": status_str,
            "notes": request.notes,
        }),
    )?;

    Ok(json!({
        "ok": true,
        "security_status": status_str,
    }))
}

pub fn update_skill_security_status(
    auth: &AuthContext,
    repo_domain: &str,
    repo_path_slug: &str,
    flock_slug: &str,
    skill_slug: &str,
    request: UpdateSecurityStatusRequest,
) -> Result<serde_json::Value, AppError> {
    require_staff(auth)?;
    let mut conn = db_conn()?;
    let repo_sign = format!("{repo_domain}/{repo_path_slug}");
    let flock = fetch_flock_by_slugs(&mut conn, &repo_sign, flock_slug)?;

    let status_str = security_status_to_str(request.security_status);

    let updated = diesel::update(
        skills::table
            .filter(skills::flock_id.eq(flock.id))
            .filter(skills::slug.eq(skill_slug)),
    )
    .set(skills::security_status.eq(status_str))
    .execute(&mut conn)?;

    if updated == 0 {
        return Err(AppError::NotFound(format!(
            "skill `{skill_slug}` not found in flock `{flock_slug}`"
        )));
    }

    insert_audit_log(
        &mut conn,
        Some(auth.user.id),
        "flock_skill.security_status_update",
        "flock_skill",
        None,
        json!({
            "repo_domain": repo_domain,
            "repo_path_slug": repo_path_slug,
            "flock_slug": flock_slug,
            "skill_slug": skill_slug,
            "security_status": status_str,
            "notes": request.notes,
        }),
    )?;

    Ok(json!({
        "ok": true,
        "security_status": status_str,
    }))
}

// ---------------------------------------------------------------------------
// Enhanced automated scanning pipeline
// ---------------------------------------------------------------------------

/// Input for the enhanced scan pipeline. Each skill carries its file contents.
pub struct SkillScanInput {
    pub skill_id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub license: String,
    pub version: Option<String>,
    pub metadata_json: Option<String>,
    pub frontmatter_always: Option<bool>,
    /// All files belonging to this skill (path + content).
    pub files: Vec<FileContent>,
    /// The skill_version id this scan is for.
    pub version_id: Option<Uuid>,
}

/// Context for the scan pipeline that ties results to a specific git commit.
pub struct ScanContext {
    pub commit_hash: Option<String>,
}

/// Run all automated scans (static + legacy content_analysis + license_audit).
///
/// Returns the worst verdict as a string. Also spawns an async LLM evaluation
/// if AI is configured.
pub fn run_automated_scans(
    conn: &mut PgConnection,
    flock_id: Uuid,
    skills: &[SkillRow],
) -> Result<String, AppError> {
    run_automated_scans_with_files(conn, flock_id, skills, &[], None)
}

/// Enhanced version of `run_automated_scans` that also accepts file contents
/// for deep static scanning and async LLM evaluation.
///
/// The enhanced pipeline (static file scanning + LLM eval) only runs when
/// `SAVHUB_SECURITY_SCAN=true`. Otherwise only the legacy content_analysis
/// and license_audit scanners execute.
pub fn run_automated_scans_with_files(
    conn: &mut PgConnection,
    flock_id: Uuid,
    skills: &[SkillRow],
    skill_files: &[SkillScanInput],
    scan_ctx: Option<&ScanContext>,
) -> Result<String, AppError> {
    let commit_hash = scan_ctx.and_then(|c| c.commit_hash.clone());

    // Skip if this commit_hash was already scanned for this flock.
    if let Some(ref hash) = commit_hash {
        if !hash.is_empty() {
            let already_scanned = security_scans::table
                .filter(security_scans::target_type.eq("flock"))
                .filter(security_scans::target_id.eq(flock_id))
                .filter(security_scans::commit_hash.eq(hash))
                .select(security_scans::id)
                .first::<Uuid>(conn)
                .optional()?
                .is_some();
            if already_scanned {
                tracing::info!(
                    "[security] flock {} already scanned at commit {} — skipping",
                    flock_id,
                    &hash[..hash.len().min(8)],
                );
                return Ok("clean".to_string());
            }
        }
    }

    let scan_enabled = crate::state::app_state().config.security_scan_enabled;
    let mut worst_verdict = ModerationVerdict::Clean;
    let mut static_scan_ran = false;

    tracing::info!(
        "[security] running scans for flock {} ({} skills, scan_enabled={}, files={})",
        flock_id,
        skills.len(),
        scan_enabled,
        skill_files.len(),
    );

    // Build a lookup from skill slug → file contents.
    // Only populate when security scanning is enabled.
    let file_map: std::collections::HashMap<&str, &SkillScanInput> = if scan_enabled {
        skill_files
            .iter()
            .map(|sf| (sf.slug.as_str(), sf))
            .collect()
    } else {
        std::collections::HashMap::new()
    };

    for skill in skills {
        // ----- Legacy content analysis (on name/description text) -----
        let content_result = content_analysis_scan(
            &skill.name,
            skill.description.as_deref().unwrap_or_default(),
            skill.description.as_deref(),
        );
        let severity = match content_result.as_str() {
            "fail" => Some("high"),
            "warn" => Some("medium"),
            _ => None,
        };
        let skill_version_id = file_map
            .get(skill.slug.as_str())
            .and_then(|sf| sf.version_id);

        diesel::insert_into(security_scans::table)
            .values(NewSecurityScanRow {
                id: Uuid::now_v7(),
                target_type: "flock_skill".to_string(),
                target_id: skill.id,
                scan_module: "content_analysis".to_string(),
                result: content_result,
                severity: severity.map(ToString::to_string),
                details: json!({}),
                scanned_by_user_id: None,
                created_at: Utc::now(),
                version_id: skill_version_id,
                commit_hash: commit_hash.clone().unwrap_or_default(),
            })
            .execute(conn)?;

        // ----- License audit -----
        let license_result = license_audit_scan(skill.license.as_deref().unwrap_or(""));
        let license_severity = if license_result == "fail" {
            Some("medium")
        } else {
            None
        };
        diesel::insert_into(security_scans::table)
            .values(NewSecurityScanRow {
                id: Uuid::now_v7(),
                target_type: "flock_skill".to_string(),
                target_id: skill.id,
                scan_module: "license_audit".to_string(),
                result: license_result,
                severity: license_severity.map(ToString::to_string),
                details: json!({}),
                scanned_by_user_id: None,
                created_at: Utc::now(),
                version_id: skill_version_id,
                commit_hash: commit_hash.clone().unwrap_or_default(),
            })
            .execute(conn)?;

        // ----- Static scan (on actual file contents) -----
        if let Some(scan_input) = file_map.get(skill.slug.as_str()) {
            static_scan_ran = true;
            let input = ScanInput {
                slug: &scan_input.slug,
                display_name: &scan_input.name,
                summary: scan_input.description.as_deref(),
                files: &scan_input.files,
                metadata_json: scan_input.metadata_json.as_deref(),
                frontmatter_always: scan_input.frontmatter_always,
            };
            let static_result = security_scan::run_static_scan(&input);

            // Store static scan result
            let static_severity = match static_result.verdict {
                ModerationVerdict::Malicious => Some("high"),
                ModerationVerdict::Suspicious => Some("medium"),
                ModerationVerdict::Clean => None,
            };
            diesel::insert_into(security_scans::table)
                .values(NewSecurityScanRow {
                    id: Uuid::now_v7(),
                    target_type: "flock_skill".to_string(),
                    target_id: skill.id,
                    scan_module: "static_moderation".to_string(),
                    result: static_result.verdict.to_string(),
                    severity: static_severity.map(ToString::to_string),
                    details: json!({
                        "reason_codes": static_result.reason_codes,
                        "findings": static_result.findings,
                        "summary": static_result.summary,
                        "engine_version": static_result.engine_version,
                    }),
                    scanned_by_user_id: None,
                    created_at: Utc::now(),
                    version_id: scan_input.version_id,
                    commit_hash: commit_hash.clone().unwrap_or_default(),
                })
                .execute(conn)?;

            // Track worst verdict
            match static_result.verdict {
                ModerationVerdict::Malicious => worst_verdict = ModerationVerdict::Malicious,
                ModerationVerdict::Suspicious if worst_verdict == ModerationVerdict::Clean => {
                    worst_verdict = ModerationVerdict::Suspicious;
                }
                _ => {}
            }

            // Static scan passed → "partially". AI eval will upgrade to "verified" if enabled.
            let ai_enabled = {
                let cfg = &crate::state::app_state().config;
                cfg.ai_provider.is_some() && cfg.ai_api_key.is_some()
            };
            let security_status = match static_result.verdict {
                ModerationVerdict::Malicious => "malicious",
                ModerationVerdict::Suspicious => "suspicious",
                ModerationVerdict::Clean => "partially",
            };
            diesel::update(skills::table.find(skill.id))
                .set(skills::security_status.eq(security_status))
                .execute(conn)?;

            // Write consolidated scan_summary to the skill_version row
            if let Some(vid) = scan_input.version_id {
                let scan_summary = build_initial_scan_summary(&static_result);
                if let Ok(val) = serde_json::to_value(&scan_summary) {
                    let _ = diesel::update(skill_versions::table.find(vid))
                        .set(skill_versions::scan_summary.eq(Some(val)))
                        .execute(conn);
                }
            }

            // ----- Spawn async LLM evaluation (only if AI is configured) -----
            if ai_enabled {
                schedule_llm_eval(skill, flock_id, scan_input, &static_result);
            }
        }
    }

    // If no static scan ran (scan not enabled or no file contents provided),
    // leave skills at their current status (default "unscanned").
    if !static_scan_ran {
        tracing::info!(
            "[security] no static scan ran for flock {} — keeping current status",
            flock_id,
        );
    } else {
        tracing::info!(
            "[security] static scan completed for flock {}: worst_verdict={}",
            flock_id,
            worst_verdict,
        );
    }

    // Record a flock-level scan summary
    let worst_str = worst_verdict.to_string();
    diesel::insert_into(security_scans::table)
        .values(NewSecurityScanRow {
            id: Uuid::now_v7(),
            target_type: "flock".to_string(),
            target_id: flock_id,
            scan_module: "aggregate".to_string(),
            result: worst_str.clone(),
            severity: None,
            details: json!({
                "skill_count": skills.len(),
                "verdict": worst_str,
            }),
            scanned_by_user_id: None,
            created_at: Utc::now(),
            version_id: None,
            commit_hash: commit_hash.clone().unwrap_or_default(),
        })
        .execute(conn)?;

    // Update flock security_status. Static clean → "partially".
    // AI eval will upgrade to "verified" later if enabled.
    let flock_status = match worst_verdict {
        ModerationVerdict::Malicious => "malicious",
        ModerationVerdict::Suspicious => "suspicious",
        ModerationVerdict::Clean => "partially",
    };
    diesel::update(flocks::table.find(flock_id))
        .set(flocks::security_status.eq(flock_status))
        .execute(conn)?;

    Ok(worst_str)
}

/// Spawn the LLM security evaluation as an async background task.
fn schedule_llm_eval(
    skill: &SkillRow,
    flock_id: Uuid,
    scan_input: &SkillScanInput,
    static_result: &StaticScanResult,
) {
    // Only schedule if AI provider is configured
    let config = &crate::state::app_state().config;
    if config.ai_provider.is_none() || config.ai_api_key.is_none() {
        tracing::info!(
            "[security] AI provider not configured — skipping LLM eval for {}",
            skill.slug,
        );
        return;
    }

    // Find SKILL.md content
    let skill_md_content = scan_input
        .files
        .iter()
        .find(|f| {
            let lower = f.path.to_lowercase();
            lower == "skill.md" || lower == "skills.md" || lower.ends_with("/skill.md")
        })
        .map(|f| f.content.clone())
        .unwrap_or_default();

    if skill_md_content.is_empty() {
        tracing::debug!(
            "[security] skipping LLM eval for {}: no SKILL.md",
            skill.slug
        );
        return;
    }

    // Detect injection patterns across all content
    let all_content: String = scan_input
        .files
        .iter()
        .map(|f| f.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let injection_signals = detect_injection_patterns(&all_content);

    // Build eval context
    let eval_ctx = SkillEvalContext {
        slug: skill.slug.clone(),
        display_name: skill.name.clone(),
        summary: skill.description.clone(),
        version: skill.version.clone(),
        skill_md_content,
        file_contents: scan_input
            .files
            .iter()
            .filter(|f| {
                let lower = f.path.to_lowercase();
                lower != "skill.md" && lower != "skills.md"
            })
            .map(|f| FileEntry {
                path: f.path.clone(),
                content: f.content.clone(),
            })
            .collect(),
        file_manifest: scan_input
            .files
            .iter()
            .map(|f| FileManifestEntry {
                path: f.path.clone(),
                size: f.content.len(),
            })
            .collect(),
        injection_signals,
        metadata_json: scan_input.metadata_json.clone(),
        frontmatter_always: scan_input.frontmatter_always,
    };

    let skill_id = skill.id;
    let skill_slug = skill.slug.clone();
    let static_result_clone = static_result.clone();
    let version_id = scan_input.version_id;

    tokio::spawn(async move {
        match llm_eval::evaluate_skill_with_llm(
            skill_id,
            flock_id,
            eval_ctx,
            Some(&static_result_clone),
        )
        .await
        {
            Ok(result) => {
                // Update security_status based on combined verdict
                let (combined_verdict, _codes, _summary) =
                    build_moderation_verdict(Some(&static_result_clone), Some(&result.verdict));
                let final_status = match combined_verdict {
                    ModerationVerdict::Malicious => "malicious",
                    ModerationVerdict::Suspicious => "suspicious",
                    ModerationVerdict::Clean => "verified",
                };
                if let Ok(mut conn) = db_conn() {
                    let _ = diesel::update(skills::table.find(skill_id))
                        .set(skills::security_status.eq(final_status))
                        .execute(&mut conn);
                    let _ = diesel::update(flocks::table.find(flock_id))
                        .set(flocks::security_status.eq(final_status))
                        .execute(&mut conn);

                    // Merge LLM verdict into the version scan_summary
                    if let Some(vid) = version_id {
                        let llm_verdict = match result.verdict.as_str() {
                            "malicious" => ScanVerdict::Malicious,
                            "suspicious" => ScanVerdict::Suspicious,
                            "benign" | "clean" => ScanVerdict::Benign,
                            _ => ScanVerdict::Pending,
                        };
                        merge_llm_into_scan_summary(
                            &mut conn,
                            vid,
                            llm_verdict,
                            Some(&result.verdict),
                        );
                    }
                }
                tracing::info!(
                    "[security] LLM eval complete for {}: {} → status={}",
                    skill_slug,
                    result.verdict,
                    final_status,
                );
            }
            Err(e) => {
                tracing::error!("[security] LLM eval failed for {}: {}", skill_slug, e);
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Scan summary helpers
// ---------------------------------------------------------------------------

/// Build an initial `VersionScanSummary` from the static scan result alone.
/// The LLM verdict will be merged in asynchronously later.
fn build_initial_scan_summary(static_result: &StaticScanResult) -> VersionScanSummary {
    let static_status = match static_result.verdict {
        ModerationVerdict::Clean => "clean",
        ModerationVerdict::Suspicious => "suspicious",
        ModerationVerdict::Malicious => "malicious",
    };
    VersionScanSummary {
        sha256: None,
        virustotal: None,
        llm_analysis: None,
        static_scan: Some(SharedStaticScanResult {
            status: static_status.to_string(),
            engine_version: Some(static_result.engine_version.clone()),
            summary: Some(static_result.summary.clone()),
            findings: static_result
                .findings
                .iter()
                .map(|f| SharedStaticScanFinding {
                    code: f.code.clone(),
                    severity: format!("{:?}", f.severity).to_lowercase(),
                    file: Some(f.file.clone()),
                    line: Some(f.line as i32),
                    message: Some(f.message.clone()),
                })
                .collect(),
            reason_codes: static_result.reason_codes.clone(),
            checked_at: Some(Utc::now()),
        }),
    }
}

/// Merge LLM evaluation results into an existing scan_summary on a version row.
pub fn merge_llm_into_scan_summary(
    conn: &mut diesel::PgConnection,
    version_id: Uuid,
    verdict: ScanVerdict,
    summary: Option<&str>,
) {
    let llm_result = shared::LlmScanResult {
        verdict,
        status: match verdict {
            ScanVerdict::Benign => "clean".to_string(),
            ScanVerdict::Suspicious => "suspicious".to_string(),
            ScanVerdict::Malicious => "malicious".to_string(),
            ScanVerdict::Pending => "pending".to_string(),
        },
        confidence: None,
        model: None,
        summary: summary.map(ToString::to_string),
        guidance: None,
        dimensions: vec![],
        checked_at: Some(Utc::now()),
    };

    if let Ok(llm_json) = serde_json::to_value(&llm_result) {
        // Read existing scan_summary, merge llm_analysis, and write back.
        let existing: Option<serde_json::Value> = skill_versions::table
            .find(version_id)
            .select(skill_versions::scan_summary)
            .first::<Option<serde_json::Value>>(conn)
            .ok()
            .flatten();

        let mut obj = match existing {
            Some(serde_json::Value::Object(m)) => m,
            _ => serde_json::Map::new(),
        };
        obj.insert("llm_analysis".to_string(), llm_json);

        let _ = diesel::update(skill_versions::table.find(version_id))
            .set(skill_versions::scan_summary.eq(Some(serde_json::Value::Object(obj))))
            .execute(conn);
    }
}

// ---------------------------------------------------------------------------
// Query endpoints
// ---------------------------------------------------------------------------

pub fn list_flock_scans_by_slugs(
    repo_domain: &str,
    repo_path_slug: &str,
    flock_slug: &str,
) -> Result<SecurityScanListResponse, AppError> {
    let mut conn = db_conn()?;
    let repo_sign = format!("{repo_domain}/{repo_path_slug}");
    let flock = fetch_flock_by_slugs(&mut conn, &repo_sign, flock_slug)?;
    list_security_scans("flock", flock.id)
}

pub fn list_security_scans(
    target_type_val: &str,
    target_id_val: Uuid,
) -> Result<SecurityScanListResponse, AppError> {
    let mut conn = db_conn()?;
    let rows = security_scans::table
        .filter(security_scans::target_type.eq(target_type_val))
        .filter(security_scans::target_id.eq(target_id_val))
        .order(security_scans::created_at.desc())
        .select(SecurityScanRow::as_select())
        .load::<SecurityScanRow>(&mut conn)?;

    let user_ids: Vec<Uuid> = rows.iter().filter_map(|r| r.scanned_by_user_id).collect();
    let users = load_users_map(&mut conn, user_ids)?;

    let scans = rows
        .into_iter()
        .map(|row| SecurityScanDto {
            id: row.id,
            scan_module: row.scan_module,
            result: row.result,
            severity: row.severity,
            details: row.details,
            scanned_by: row
                .scanned_by_user_id
                .and_then(|id| users.get(&id))
                .map(user_summary_from_row),
            created_at: row.created_at,
        })
        .collect();

    Ok(SecurityScanListResponse { scans })
}

// ---------------------------------------------------------------------------
// Legacy scanners (kept for backward compatibility)
// ---------------------------------------------------------------------------

fn content_analysis_scan(name: &str, summary: &str, description: Option<&str>) -> String {
    let all_text = format!("{} {} {}", name, summary, description.unwrap_or_default());
    let lower = all_text.to_lowercase();

    let suspicious_patterns = [
        "eval(",
        "exec(",
        "system(",
        "base64_decode",
        "subprocess.call",
        "os.system(",
        "rm -rf",
        "<script>",
        "javascript:",
        "curl | sh",
        "curl | bash",
        "wget | sh",
        "wget | bash",
    ];

    for pattern in &suspicious_patterns {
        if lower.contains(pattern) {
            return "fail".to_string();
        }
    }

    let injection_patterns = [
        "ignore previous instructions",
        "ignore all previous",
        "disregard the above",
        "forget your instructions",
    ];

    for pattern in &injection_patterns {
        if lower.contains(pattern) {
            return "warn".to_string();
        }
    }

    for word in all_text.split_whitespace() {
        if word.len() > 100
            && word
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=')
        {
            return "warn".to_string();
        }
    }

    "pass".to_string()
}

fn license_audit_scan(license: &str) -> String {
    let known_spdx = [
        "MIT",
        "Apache-2.0",
        "GPL-2.0",
        "GPL-3.0",
        "LGPL-2.1",
        "LGPL-3.0",
        "BSD-2-Clause",
        "BSD-3-Clause",
        "ISC",
        "MPL-2.0",
        "AGPL-3.0",
        "Unlicense",
        "CC0-1.0",
        "CC-BY-4.0",
        "CC-BY-SA-4.0",
        "BSL-1.0",
        "0BSD",
        "Artistic-2.0",
        "Zlib",
        "PSF-2.0",
        "GPL-2.0-only",
        "GPL-3.0-only",
        "AGPL-3.0-only",
        "GPL-2.0-or-later",
        "GPL-3.0-or-later",
        "AGPL-3.0-or-later",
        "LGPL-2.1-only",
        "LGPL-3.0-only",
        "LGPL-2.1-or-later",
        "LGPL-3.0-or-later",
    ];

    let trimmed = license.trim();
    if trimmed.is_empty() {
        return "fail".to_string();
    }

    let parts: Vec<&str> = trimmed
        .split(|c: char| c == '(' || c == ')' || c.is_whitespace())
        .filter(|s| !s.is_empty() && *s != "OR" && *s != "AND" && *s != "WITH")
        .collect();

    for part in &parts {
        if !known_spdx
            .iter()
            .any(|spdx| spdx.eq_ignore_ascii_case(part))
        {
            return "warn".to_string();
        }
    }

    "pass".to_string()
}

// ---------------------------------------------------------------------------
// Status helpers
// ---------------------------------------------------------------------------

pub fn security_status_to_str(status: SecurityStatus) -> &'static str {
    match status {
        SecurityStatus::Unscanned => "unscanned",
        SecurityStatus::Partially => "partially",
        SecurityStatus::Verified => "verified",
        SecurityStatus::Suspicious => "suspicious",
        SecurityStatus::Malicious => "malicious",
    }
}

pub fn parse_security_status(value: &str) -> SecurityStatus {
    match value {
        "partially" => SecurityStatus::Partially,
        "verified" => SecurityStatus::Verified,
        "suspicious" => SecurityStatus::Suspicious,
        "malicious" => SecurityStatus::Malicious,
        _ => SecurityStatus::Unscanned,
    }
}
