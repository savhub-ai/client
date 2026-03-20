use std::collections::HashSet;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use reqwest::Url;
use reqwest::header::{ACCEPT, USER_AGENT};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zip::ZipArchive;

use crate::config::Config;
use crate::error::AppError;
use crate::state::app_state;
use shared::{
    CatalogSource, CompatibilityMetadata, FlockDocument, FlockMetadata, ImportedSkillMetadata,
    ImportedSkillRecord, RegistryGitReference, RegistryMaintainer, RepoDocument,
};

use super::helpers::{extract_summary, parse_frontmatter};

const GITHUB_API_BASE: &str = "https://api.github.com/";

/// Write a base64-encoded SSH key to a temporary file and return the
/// `GIT_SSH_COMMAND` value that tells git to use it.
/// The file is written to the system temp dir with restrictive permissions.
fn ssh_command_for_key(base64_key: &str) -> Option<(PathBuf, String)> {
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(base64_key.trim())
        .ok()?;
    let key_path = std::env::temp_dir().join(format!("savhub-ssh-key-{}", Uuid::now_v7().simple()));
    fs::write(&key_path, &decoded).ok()?;
    // On Unix, restrict permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600));
    }
    let cmd = format!(
        "ssh -i {} -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null",
        key_path.display()
    );
    Some((key_path, cmd))
}

/// Copy a key file to a temp location with restrictive permissions and return
/// the `GIT_SSH_COMMAND` value. SSH refuses keys with overly permissive modes,
/// so we copy rather than use the mount directly.
fn ssh_command_for_key_file(path: &str) -> Option<String> {
    let src = Path::new(path);
    let key_data = fs::read(src).ok()?;
    let key_path = std::env::temp_dir().join(format!("savhub-ssh-key-{}", Uuid::now_v7().simple()));
    fs::write(&key_path, &key_data).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600));
    }
    Some(format!(
        "ssh -i {} -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null",
        key_path.display()
    ))
}

/// Resolve the SSH command string from config (key file takes priority over base64 key).
fn resolve_ssh_command() -> Option<String> {
    let config = &app_state().config;
    // Prefer key file (e.g. mounted via Docker volume)
    if let Some(ref key_file) = config.registry_git_ssh_key_file {
        if Path::new(key_file).exists() {
            if let Some(cmd) = ssh_command_for_key_file(key_file) {
                return Some(cmd);
            }
            tracing::warn!("failed to prepare SSH key from file {key_file}, falling back");
        } else {
            tracing::warn!(
                "SAVHUB_REGISTRY_GIT_SSH_KEY_FILE={key_file} does not exist, falling back"
            );
        }
    }
    // Fall back to base64-encoded key
    if let Some(ref key) = config.registry_git_ssh_key {
        if let Some((_path, ssh_cmd)) = ssh_command_for_key(key) {
            return Some(ssh_cmd);
        }
    }
    None
}

/// Apply SSH key env to a Command if configured.
fn apply_ssh_env(cmd: &mut std::process::Command) {
    if let Some(ssh_cmd) = resolve_ssh_command() {
        cmd.env("GIT_SSH_COMMAND", &ssh_cmd);
    }
}

/// Apply SSH key env to an async Command if configured.
fn apply_ssh_env_async(cmd: &mut tokio::process::Command) {
    if let Some(ssh_cmd) = resolve_ssh_command() {
        cmd.env("GIT_SSH_COMMAND", &ssh_cmd);
    }
}

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

/// Synchronous version of [`setup_windows_sparse_checkout`].
fn setup_windows_sparse_checkout_sync(repo_dir: &Path) {
    if !cfg!(windows) {
        return;
    }
    let _ = std::process::Command::new("git")
        .args(["config", "core.protectNTFS", "false"])
        .current_dir(repo_dir)
        .output();
    let _ = std::process::Command::new("git")
        .args(["config", "core.sparseCheckout", "true"])
        .current_dir(repo_dir)
        .output();
    let info_dir = repo_dir.join(".git").join("info");
    let _ = fs::create_dir_all(&info_dir);
    let _ = fs::write(info_dir.join("sparse-checkout"), "*\n!*:*\n");
}

fn normalize_registry_branch_target(branch: &str) -> Option<String> {
    let branch = branch.trim();
    if branch.is_empty()
        || branch.eq_ignore_ascii_case("HEAD")
        || branch.eq_ignore_ascii_case("origin/HEAD")
    {
        return None;
    }
    Some(branch.to_string())
}

fn parse_git_stdout_lines(stdout: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn select_registry_remote_branch(remote_head: Option<&str>, refs: &[String]) -> String {
    if let Some(branch) = remote_head.and_then(normalize_registry_branch_target) {
        return branch;
    }

    let normalized_refs: Vec<String> = refs
        .iter()
        .filter_map(|reference| normalize_registry_branch_target(reference))
        .collect();

    for candidate in ["origin/main", "origin/master", "main", "master"] {
        if normalized_refs
            .iter()
            .any(|reference| reference == candidate)
        {
            return candidate.to_string();
        }
    }

    normalized_refs
        .iter()
        .find(|reference| reference.starts_with("origin/"))
        .cloned()
        .or_else(|| normalized_refs.into_iter().next())
        .unwrap_or_else(|| "main".to_string())
}

async fn resolve_registry_remote_branch_async(registry_path: &Path) -> String {
    let remote_head = tokio::process::Command::new("git")
        .args(["symbolic-ref", "refs/remotes/origin/HEAD", "--short"])
        .current_dir(registry_path)
        .output()
        .await
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(ToOwned::to_owned)
        });

    let refs = tokio::process::Command::new("git")
        .args([
            "for-each-ref",
            "--format=%(refname:short)",
            "refs/remotes/origin",
            "refs/heads",
        ])
        .current_dir(registry_path)
        .output()
        .await
        .ok()
        .filter(|output| output.status.success())
        .map(|output| parse_git_stdout_lines(&output.stdout))
        .unwrap_or_default();

    select_registry_remote_branch(remote_head.as_deref(), &refs)
}

fn resolve_registry_remote_branch(registry_path: &Path) -> String {
    let remote_head = std::process::Command::new("git")
        .args(["symbolic-ref", "refs/remotes/origin/HEAD", "--short"])
        .current_dir(registry_path)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(ToOwned::to_owned)
        });

    let refs = std::process::Command::new("git")
        .args([
            "for-each-ref",
            "--format=%(refname:short)",
            "refs/remotes/origin",
            "refs/heads",
        ])
        .current_dir(registry_path)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| parse_git_stdout_lines(&output.stdout))
        .unwrap_or_default();

    select_registry_remote_branch(remote_head.as_deref(), &refs)
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

#[derive(Debug, Serialize)]
struct RegistryRepoFile<'a> {
    sign: &'a str,
    name: &'a str,
    description: &'a str,
    git_url: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    git_rev: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    git_branch: Option<&'a str>,
    #[serde(skip_serializing_if = "is_public_visibility")]
    visibility: shared::RegistryVisibility,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    verified: bool,
    #[serde(flatten)]
    metadata: &'a shared::RepoMetadata,
}

fn is_public_visibility(v: &shared::RegistryVisibility) -> bool {
    matches!(v, shared::RegistryVisibility::Public)
}

#[derive(Debug, Serialize)]
struct RegistryFlockFile {
    sign: String,
    repo: String,
    name: String,
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    status: shared::RegistryStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    visibility: Option<shared::RegistryVisibility>,
    license: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    categories: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    keywords: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    maintainers: Vec<RegistryMaintainer>,
    #[serde(skip_serializing_if = "is_default_compat")]
    compatibility: CompatibilityMetadata,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    featured_skills: Vec<String>,
    #[serde(skip_serializing_if = "indexmap::IndexMap::is_empty")]
    links: indexmap::IndexMap<String, String>,
    #[serde(skip_serializing_if = "is_default_security")]
    security: shared::SecuritySummary,
}

fn is_default_security(s: &shared::SecuritySummary) -> bool {
    *s == shared::SecuritySummary::default()
}

fn is_default_compat(c: &CompatibilityMetadata) -> bool {
    *c == CompatibilityMetadata::default()
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Stages registry files to a temp directory, then commits all at once.
pub(crate) struct StagedRegistryWriter {
    temp_dir: PathBuf,
}

impl StagedRegistryWriter {
    pub fn new() -> Result<Self, AppError> {
        let temp_dir = std::env::temp_dir().join(format!(
            "savhub-staged-registry-{}",
            Uuid::now_v7().simple()
        ));
        fs::create_dir_all(&temp_dir).map_err(|e| {
            AppError::Internal(format!("failed to create staged registry dir: {e}"))
        })?;
        Ok(Self { temp_dir })
    }

    /// Write repo + flock + skills JSON files to the temp staging area.
    pub fn stage_flock(
        &mut self,
        repo: &RepoDocument,
        path_slug: &str,
        flock: &FlockDocument,
        flock_slug: &str,
        skills: &[ImportedSkillRecord],
    ) -> Result<(), AppError> {
        let (domain, _) = repo.sign.split_once('/').unwrap_or((&repo.sign, ""));
        let repo_dir = self.temp_dir.join("data").join(domain).join(path_slug);
        let flock_dir = repo_dir.join(flock_slug);

        write_json_file(
            &repo_dir.join("repo.json"),
            &RegistryRepoFile {
                sign: &repo.sign,
                name: &repo.name,
                description: &repo.description,
                git_url: &repo.git_url,
                git_rev: repo.git_rev.as_deref(),
                git_branch: repo.git_branch.as_deref(),
                visibility: repo.visibility,
                verified: repo.verified,
                metadata: &repo.metadata,
            },
        )?;
        write_json_file(
            &flock_dir.join("flock.json"),
            &RegistryFlockFile::from_flock(domain, path_slug, flock_slug, flock),
        )?;
        write_json_file(&flock_dir.join("skills.json"), skills)?;

        Ok(())
    }

    /// Copy all staged files to the real registry checkout, then git add+commit+push.
    pub fn commit_to_registry(&self, message: &str) -> Result<(), AppError> {
        let root = app_state().config.registry_repo_path();
        if !root.join(".git").is_dir() {
            return Err(AppError::Internal(format!(
                "registry checkout `{}` does not look like the savfox registry repo",
                root.display()
            )));
        }

        pull_registry_latest(&root);

        // Delete old repo directories that are about to be overwritten, so stale
        // flock/skill data from previous indexing runs is cleaned up.
        let staged_data = self.temp_dir.join("data");
        if staged_data.is_dir() {
            if let Ok(domains) = fs::read_dir(&staged_data) {
                for domain_entry in domains.flatten() {
                    if !domain_entry.file_type().map_or(false, |t| t.is_dir()) {
                        continue;
                    }
                    if let Ok(repos) = fs::read_dir(domain_entry.path()) {
                        for repo_entry in repos.flatten() {
                            if !repo_entry.file_type().map_or(false, |t| t.is_dir()) {
                                continue;
                            }
                            // This is a data/{domain}/{path_slug} directory in the staged area.
                            // Delete the corresponding directory in the real checkout.
                            let real_repo_dir = root
                                .join("data")
                                .join(domain_entry.file_name())
                                .join(repo_entry.file_name());
                            if real_repo_dir.is_dir() {
                                tracing::info!(
                                    "removing old registry repo dir `{}` before re-populating",
                                    real_repo_dir.display()
                                );
                                let _ = fs::remove_dir_all(&real_repo_dir);
                            }
                        }
                    }
                }
            }
        }

        // Recursively copy staged files to the registry checkout
        copy_dir_recursive(&self.temp_dir, &root)?;

        commit_and_push_registry(&root, message);

        Ok(())
    }
}

impl Drop for StagedRegistryWriter {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.temp_dir);
    }
}

/// Recursively copy all files from src into dst, creating directories as needed.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), AppError> {
    let entries = match fs::read_dir(src) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };
    for entry in entries {
        let entry = entry.map_err(|e| AppError::Internal(format!("copy_dir read error: {e}")))?;
        let file_type = entry
            .file_type()
            .map_err(|e| AppError::Internal(format!("copy_dir file_type error: {e}")))?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            fs::create_dir_all(&dst_path)
                .map_err(|e| AppError::Internal(format!("copy_dir mkdir error: {e}")))?;
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            if let Some(parent) = dst_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| AppError::Internal(format!("copy_dir parent mkdir error: {e}")))?;
            }
            fs::copy(&src_path, &dst_path)
                .map_err(|e| AppError::Internal(format!("copy_dir copy error: {e}")))?;
        }
    }
    Ok(())
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

/// Ensure the registry git repo is cloned locally. Called once at startup.
/// - If `.git` exists: verify remote URL matches, then pull.
/// - If directory exists without `.git`: `git init` + add remote + fetch + reset.
/// - If directory doesn't exist: shallow clone.
pub async fn ensure_registry_repo(config: &Config) -> Result<(), AppError> {
    let registry_path = config.registry_repo_path();
    let git_dir = registry_path.join(".git");
    let auth_url = config.registry_git_url_with_auth();

    if git_dir.is_dir() {
        // Already a git repo — verify remote and pull (use auth URL so push works)
        verify_or_set_remote(&registry_path, &auth_url).await?;
        configure_git_identity(&registry_path).await;
        tracing::info!(
            "registry repo at {}, pulling latest",
            registry_path.display()
        );
        // Fetch + reset to remote HEAD (no local changes expected at startup)
        let mut fetch_cmd = tokio::process::Command::new("git");
        fetch_cmd
            .args([
                "fetch",
                "--prune",
                "origin",
                "+refs/heads/*:refs/remotes/origin/*",
            ])
            .current_dir(&registry_path);
        apply_ssh_env_async(&mut fetch_cmd);
        let output = fetch_cmd
            .output()
            .await
            .map_err(|e| AppError::Internal(format!("failed to fetch registry repo: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::warn!("registry fetch failed (non-fatal): {stderr}");
        } else {
            let remote_branch = resolve_registry_remote_branch_async(&registry_path).await;
            setup_windows_sparse_checkout(&registry_path).await;
            let reset_output = tokio::process::Command::new("git")
                .args(["reset", "--hard", &remote_branch])
                .current_dir(&registry_path)
                .output()
                .await;
            if let Ok(o) = reset_output {
                if !o.status.success() {
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    tracing::warn!(
                        "registry reset to {remote_branch} failed (non-fatal): {stderr}"
                    );
                }
            }
        }
        return Ok(());
    }

    if registry_path.is_dir() {
        // Directory exists but is not a git repo — remove and re-clone
        tracing::info!(
            "registry directory exists at {} without .git, removing and re-cloning",
            registry_path.display()
        );
        fs::remove_dir_all(&registry_path)
            .map_err(|e| AppError::Internal(format!("failed to remove registry directory: {e}")))?;
    }

    // Fresh clone
    tracing::info!(
        "cloning registry repo {} -> {}",
        config.registry_git_url,
        registry_path.display()
    );
    fs::create_dir_all(registry_path.parent().unwrap_or(&registry_path)).map_err(|e| {
        AppError::Internal(format!("failed to create registry parent directory: {e}"))
    })?;

    let mut clone_cmd = tokio::process::Command::new("git");
    clone_cmd.args([
        "clone",
        "--depth",
        "1",
        &auth_url,
        &registry_path.to_string_lossy(),
    ]);
    apply_ssh_env_async(&mut clone_cmd);
    let output = clone_cmd
        .output()
        .await
        .map_err(|e| AppError::Internal(format!("failed to clone registry repo: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if cfg!(windows) && stderr.contains("Clone succeeded, but checkout failed") {
            tracing::warn!(
                "registry clone checkout failed (invalid Windows filename), \
                 configuring sparse-checkout to skip them"
            );
            setup_windows_sparse_checkout(&registry_path).await;
            let _ = tokio::process::Command::new("git")
                .args(["reset", "--hard", "HEAD"])
                .current_dir(&registry_path)
                .output()
                .await;
        } else {
            return Err(AppError::Internal(format!(
                "git clone of registry repo failed: {stderr}"
            )));
        }
    }

    configure_git_identity(&registry_path).await;

    tracing::info!("registry repo cloned successfully");
    Ok(())
}

/// Configure git identity for registry commits. Called on every startup
/// so the values stay up to date even if the repo already existed.
async fn configure_git_identity(registry_path: &Path) {
    let _ = tokio::process::Command::new("git")
        .args(["config", "user.name", "savhub-bot"])
        .current_dir(registry_path)
        .output()
        .await;
    let _ = tokio::process::Command::new("git")
        .args(["config", "user.email", "aston@sonc.ai"])
        .current_dir(registry_path)
        .output()
        .await;
}

/// Ensure the origin remote URL matches the expected URL.
async fn verify_or_set_remote(path: &Path, expected_url: &str) -> Result<(), AppError> {
    let output = tokio::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(path)
        .output()
        .await
        .map_err(|e| AppError::Internal(format!("failed to get registry remote url: {e}")))?;

    let current_url = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if current_url != expected_url {
        tracing::info!(
            "registry remote URL mismatch (got {current_url}), setting to {expected_url}"
        );
        tokio::process::Command::new("git")
            .args(["remote", "set-url", "origin", expected_url])
            .current_dir(path)
            .output()
            .await
            .map_err(|e| AppError::Internal(format!("failed to set registry remote url: {e}")))?;
    }
    Ok(())
}

pub fn sync_registry_checkout(
    repo: &RepoDocument,
    path_slug: &str,
    flock: &FlockDocument,
    flock_slug: &str,
    skills: &[ImportedSkillRecord],
) -> Result<(), AppError> {
    let _lock = app_state()
        .registry_lock
        .lock()
        .expect("registry lock poisoned");

    let root = app_state().config.registry_repo_path();
    if !root.join(".git").is_dir() {
        return Err(AppError::Internal(format!(
            "registry checkout `{}` does not look like the savfox registry repo",
            root.display()
        )));
    }

    // Pull latest remote before writing, to minimize conflicts on push
    pull_registry_latest(&root);

    let (domain, _) = repo.sign.split_once('/').unwrap_or((&repo.sign, ""));
    let repo_dir = root.join("data").join(domain).join(path_slug);
    let flock_dir = repo_dir.join(flock_slug);

    // Remove old flock directory so stale files (e.g. renamed/deleted skills) are cleaned up
    if flock_dir.is_dir() {
        let _ = fs::remove_dir_all(&flock_dir);
    }

    write_json_file(
        &repo_dir.join("repo.json"),
        &RegistryRepoFile {
            sign: &repo.sign,
            name: &repo.name,
            description: &repo.description,
            git_url: &repo.git_url,
            git_rev: repo.git_rev.as_deref(),
            git_branch: repo.git_branch.as_deref(),
            visibility: repo.visibility,
            verified: repo.verified,
            metadata: &repo.metadata,
        },
    )?;
    write_json_file(
        &flock_dir.join("flock.json"),
        &RegistryFlockFile::from_flock(domain, path_slug, flock_slug, flock),
    )?;
    write_json_file(&flock_dir.join("skills.json"), skills)?;

    // Git add + commit + push (blocking, best-effort)
    commit_and_push_registry(
        &root,
        &format!(
            "sync: {}/{} ({} skills)",
            repo.sign,
            flock_slug,
            skills.len()
        ),
    );

    Ok(())
}

/// When a git remote returns an HTTP redirect (301/302), the repo has been
/// moved or renamed.  This function updates the database (repos, flocks,
/// skills) and renames the corresponding registry folder so everything
/// points to the new URL.
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
        "applying repo redirect: updating DB and registry folder"
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

    // 5. Rename registry folder: data/{old_domain}/{old_path_slug} → data/{new_domain}/{new_path_slug}
    let registry_root = app_state().config.registry_repo_path();
    let old_dir = registry_root
        .join("data")
        .join(&old_domain)
        .join(&old_path_slug);
    let new_dir = registry_root
        .join("data")
        .join(&new_domain)
        .join(&new_path_slug);

    if old_dir.is_dir() {
        if let Some(parent) = new_dir.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Err(e) = fs::rename(&old_dir, &new_dir) {
            tracing::warn!(
                "failed to rename registry folder `{}` -> `{}`: {e}",
                old_dir.display(),
                new_dir.display()
            );
        } else {
            tracing::info!(
                "renamed registry folder `{}` -> `{}`",
                old_dir.display(),
                new_dir.display()
            );
            // Commit the rename to git
            commit_and_push_registry(
                &registry_root,
                &format!("redirect: {old_sign} -> {new_sign}"),
            );
        }
    }

    Ok(())
}

/// Best-effort pull latest from remote before writing files.
fn pull_registry_latest(registry_path: &Path) {
    let mut fetch_cmd = std::process::Command::new("git");
    fetch_cmd
        .args([
            "fetch",
            "--prune",
            "origin",
            "+refs/heads/*:refs/remotes/origin/*",
        ])
        .current_dir(registry_path);
    apply_ssh_env(&mut fetch_cmd);
    match fetch_cmd.output() {
        Ok(o) if !o.status.success() => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            tracing::warn!("registry pre-write fetch failed (non-fatal): {stderr}");
            return;
        }
        Err(e) => {
            tracing::warn!("registry pre-write fetch error: {e}");
            return;
        }
        _ => {}
    }
    let remote_branch = resolve_registry_remote_branch(registry_path);

    setup_windows_sparse_checkout_sync(registry_path);

    // Reset to remote HEAD — no local uncommitted changes expected before write
    let reset = std::process::Command::new("git")
        .args(["reset", "--hard", &remote_branch])
        .current_dir(registry_path)
        .output();
    match reset {
        Ok(o) if !o.status.success() => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            tracing::warn!("registry pre-write reset failed (non-fatal): {stderr}");
        }
        Err(e) => tracing::warn!("registry pre-write reset error: {e}"),
        _ => {}
    }
}

/// Best-effort git add, commit, and push for the registry repo.
fn commit_and_push_registry(registry_path: &Path, message: &str) {
    let run = |args: &[&str]| -> bool {
        let mut cmd = std::process::Command::new("git");
        cmd.args(args).current_dir(registry_path);
        apply_ssh_env(&mut cmd);
        match cmd.output() {
            Ok(output) => {
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    tracing::warn!("registry git {:?} failed: {stderr}", args.first());
                    false
                } else {
                    true
                }
            }
            Err(e) => {
                tracing::warn!("registry git {:?} error: {e}", args.first());
                false
            }
        }
    };

    if !run(&["add", "-A"]) {
        return;
    }

    // Check if there are staged changes
    let diff_output = std::process::Command::new("git")
        .args(["diff", "--cached", "--quiet"])
        .current_dir(registry_path)
        .status();
    match diff_output {
        Ok(status) if status.success() => {
            // Exit code 0 = no changes staged, nothing to commit
            return;
        }
        _ => {} // Exit code 1 = changes exist, proceed
    }

    if !run(&["commit", "-m", message]) {
        return;
    }

    if !app_state().config.registry_git_push {
        tracing::info!(
            "registry push disabled (SAVHUB_REGISTRY_GIT_PUSH=false), committed locally only"
        );
        return;
    }

    let remote_branch = resolve_registry_remote_branch(registry_path);

    // Try push; if rejected (remote ahead), fetch + rebase then push again
    if !run(&["push"]) {
        tracing::info!("registry push rejected, fetching and rebasing onto remote");
        run(&[
            "fetch",
            "--prune",
            "origin",
            "+refs/heads/*:refs/remotes/origin/*",
        ]);
        if !run(&["rebase", &remote_branch, "--strategy-option=theirs"]) {
            tracing::warn!(
                "registry rebase onto {remote_branch} failed, using soft reset fallback"
            );
            run(&["rebase", "--abort"]);
            // Soft reset to remote and re-commit (keeps changes staged)
            run(&["reset", "--soft", &remote_branch]);
            run(&["commit", "-m", message]);
        }
        if !run(&["push"]) {
            tracing::warn!("registry push still failed after rebase — will retry on next sync");
        }
    }
}

impl RegistryFlockFile {
    fn from_flock(domain: &str, path_slug: &str, flock_slug: &str, flock: &FlockDocument) -> Self {
        let repo_sign = format!("{}/{}", domain, path_slug);
        Self {
            sign: format!("{}/{}", repo_sign, flock_slug),
            repo: repo_sign,
            name: flock.name.clone(),
            description: flock.description.clone(),
            path: flock.path.clone(),
            version: flock.version.clone(),
            status: flock.status,
            visibility: flock.visibility,
            license: flock.license.clone(),
            categories: flock.metadata.categories.clone(),
            keywords: flock.metadata.keywords.clone(),
            maintainers: flock.metadata.maintainers.clone(),
            compatibility: flock.metadata.compatibility.clone(),
            featured_skills: flock.metadata.featured_skills.clone(),
            links: flock.metadata.links.clone(),
            security: flock.security.clone(),
        }
    }
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

fn write_json_file<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            AppError::Internal(format!("failed to create `{}`: {error}", parent.display()))
        })?;
    }
    let payload = serde_json::to_string_pretty(value).map_err(|error| {
        AppError::Internal(format!("failed to serialize registry metadata: {error}"))
    })?;
    fs::write(path, format!("{payload}\n")).map_err(|error| {
        AppError::Internal(format!("failed to write `{}`: {error}", path.display()))
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
    fn select_registry_remote_branch_prefers_remote_head() {
        let refs = vec!["origin/master".to_string(), "origin/main".to_string()];

        let selected = select_registry_remote_branch(Some("origin/master"), &refs);

        assert_eq!(selected, "origin/master");
    }

    #[test]
    fn select_registry_remote_branch_falls_back_to_master_when_main_is_missing() {
        let refs = vec!["origin/HEAD".to_string(), "origin/master".to_string()];

        let selected = select_registry_remote_branch(None, &refs);

        assert_eq!(selected, "origin/master");
    }

    #[test]
    fn select_registry_remote_branch_uses_first_available_remote_branch() {
        let refs = vec!["origin/develop".to_string(), "feature".to_string()];

        let selected = select_registry_remote_branch(None, &refs);

        assert_eq!(selected, "origin/develop");
    }

    #[test]
    fn select_registry_remote_branch_falls_back_to_local_main() {
        let refs = vec!["main".to_string()];

        let selected = select_registry_remote_branch(None, &refs);

        assert_eq!(selected, "main");
    }

    #[test]
    fn select_registry_remote_branch_defaults_to_main() {
        let selected = select_registry_remote_branch(None, &[]);

        assert_eq!(selected, "main");
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
