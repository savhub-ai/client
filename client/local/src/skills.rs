use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use savhub_shared::{
    BUNDLE_META_FILE, BundleMetadata, BundleSourceKind, ResourceKind, StoredBundleFile,
    load_bundle_metadata,
};
pub use savhub_shared::{LockEntry, Lockfile, RepoSkillOrigin};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use walkdir::{DirEntry, WalkDir};
use zip::ZipArchive;

use crate::utils::{sanitize_slug, title_case};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkillVersionInfo {
    pub version: Option<String>,
    pub git_commit: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LocalSkillFile {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct ZipFileSummary {
    pub path: String,
    pub size: i32,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillFolder {
    pub folder: PathBuf,
    pub slug: String,
    pub display_name: String,
    /// If this skill was found inside a flock subdirectory, the flock directory name.
    pub flock_slug: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoSkillFolder {
    pub repo_name: String,
    pub repo_root: PathBuf,
    pub skill: SkillFolder,
}

pub fn read_lockfile(workdir: &Path) -> Result<Lockfile> {
    let path = workdir.join("skills.fetched.json");
    let Ok(raw) = fs::read_to_string(&path) else {
        return Ok(Lockfile::default());
    };
    let Ok(lockfile) = serde_json::from_str::<Lockfile>(&raw) else {
        return Ok(Lockfile::default());
    };
    Ok(lockfile)
}

pub fn write_lockfile(workdir: &Path, lockfile: &Lockfile) -> Result<()> {
    let path = workdir.join("skills.fetched.json");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(lockfile)?),
    )?;
    Ok(())
}

pub fn read_repo_skill_origin(skill_folder: &Path) -> Result<Option<RepoSkillOrigin>> {
    let path = skill_folder.join(".savhub").join("repo-origin.json");
    let Ok(raw) = fs::read_to_string(&path) else {
        return Ok(None);
    };
    let Ok(origin) = serde_json::from_str::<RepoSkillOrigin>(&raw) else {
        return Ok(None);
    };
    if origin.version == 1 {
        Ok(Some(origin))
    } else {
        Ok(None)
    }
}

pub fn write_repo_skill_origin(skill_folder: &Path, origin: &RepoSkillOrigin) -> Result<()> {
    let path = skill_folder.join(".savhub").join("repo-origin.json");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(origin)?))?;
    Ok(())
}

pub fn read_skill_version_info(skill_folder: &Path) -> Result<SkillVersionInfo> {
    let mut info = SkillVersionInfo::default();

    if let Ok(raw) = fs::read_to_string(skill_folder.join("_meta.json")) {
        if let Ok(meta) = serde_json::from_str::<SkillCatalogMeta>(&raw) {
            if let Some(latest) = meta.latest {
                if let Some(version) = clean_optional_string(latest.version) {
                    info.version = Some(version);
                }
                if let Some(commit) = clean_optional_string(latest.commit) {
                    info.git_commit = normalize_git_commit(&commit).or(Some(commit));
                }
            }
        }
    }

    if info.version.is_none() || info.git_commit.is_none() {
        if let Ok(raw) = fs::read_to_string(skill_folder.join(BUNDLE_META_FILE)) {
            if let Ok(meta) = serde_json::from_str::<BundleMetadata>(&raw) {
                if info.version.is_none() {
                    let version = meta.package.version.trim();
                    if !version.is_empty() {
                        info.version = Some(version.to_string());
                    }
                }
                if info.git_commit.is_none()
                    && let Some(git) = meta.source.git
                    && git.reference.kind == BundleSourceKind::Git
                {
                    let reference = git.reference.value.trim();
                    if !reference.is_empty() {
                        info.git_commit =
                            normalize_git_commit(reference).or(Some(reference.to_string()));
                    }
                }
            }
        }
    }

    if info.version.is_none() || info.git_commit.is_none() {
        if let Some(origin) = read_repo_skill_origin(skill_folder)? {
            if info.version.is_none() {
                info.version = clean_optional_string(origin.skill_version);
            }
            if info.git_commit.is_none() {
                info.git_commit = origin
                    .repo_commit
                    .as_deref()
                    .and_then(normalize_git_commit)
                    .or(origin.repo_commit);
            }
        }
    }

    Ok(info)
}

pub fn list_publishable_files(root: &Path) -> Result<Vec<LocalSkillFile>> {
    let root = root
        .canonicalize()
        .with_context(|| format!("failed to resolve {}", root.display()))?;
    let mut files = Vec::new();
    for entry in WalkDir::new(&root)
        .into_iter()
        .filter_entry(should_visit_entry)
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let rel_path = normalize_relative_path(&root, entry.path())?;
        let Ok(content) = fs::read_to_string(entry.path()) else {
            continue;
        };
        files.push(LocalSkillFile {
            path: rel_path,
            content,
        });
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

pub fn compute_fingerprint(files: &[LocalSkillFile]) -> String {
    let mut hasher = Sha256::new();
    for file in files.iter().filter(|file| !file.path.is_empty()) {
        hasher.update(file.path.as_bytes());
        hasher.update([0]);
        hasher.update(file.content.as_bytes());
        hasher.update([0xff]);
    }
    format!("{:x}", hasher.finalize())
}

pub fn ensure_skill_marker(files: &[LocalSkillFile]) -> Result<()> {
    let found = files
        .iter()
        .any(|file| file.path.eq_ignore_ascii_case("SKILL.md"));
    if found {
        Ok(())
    } else {
        bail!("SKILL.md is required")
    }
}

pub fn load_local_skill_metadata(files: &[LocalSkillFile]) -> Result<Option<BundleMetadata>> {
    let stored_files = files
        .iter()
        .map(|file| StoredBundleFile {
            path: file.path.clone(),
            content: file.content.clone(),
            size: file.content.len() as i32,
            sha256: String::new(),
        })
        .collect::<Vec<_>>();
    load_bundle_metadata(&stored_files, ResourceKind::Skill).map_err(|error| anyhow::anyhow!(error))
}

pub fn extract_zip_to_dir(bytes: &[u8], target: &Path) -> Result<()> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))?;
    fs::create_dir_all(target)?;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        let Some(name) = file.enclosed_name() else {
            continue;
        };
        let out_path = target.join(name);
        if file.is_dir() {
            fs::create_dir_all(&out_path)?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        fs::write(out_path, buffer)?;
    }
    Ok(())
}

pub fn inspect_zip(bytes: &[u8]) -> Result<Vec<ZipFileSummary>> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))?;
    let mut files = Vec::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        if file.is_dir() {
            continue;
        }
        let Some(name) = file.enclosed_name() else {
            continue;
        };
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        let path = normalize_forward_slashes(name);
        files.push(ZipFileSummary {
            path,
            size: buffer.len() as i32,
            sha256: hash_bytes(&buffer),
        });
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

pub fn find_skill_folders(root: &Path) -> Result<Vec<SkillFolder>> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let Ok(metadata) = fs::metadata(&root) else {
        return Ok(Vec::new());
    };
    if !metadata.is_dir() {
        return Ok(Vec::new());
    }
    if let Some(skill) = skill_folder_from_path(&root) {
        return Ok(vec![skill]);
    }

    let mut results = Vec::new();
    for entry in fs::read_dir(&root)? {
        let entry = entry?;
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if !metadata.is_dir() {
            continue;
        }
        if let Some(skill) = skill_folder_from_path(&path) {
            results.push(skill);
        } else {
            // Not a skill folder — treat as a flock grouping directory
            // and scan its children one level deeper.
            let flock_name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if flock_name.is_empty() {
                continue;
            }
            if let Ok(children) = fs::read_dir(&path) {
                for child in children {
                    let Ok(child) = child else { continue };
                    let child_path = child.path();
                    let Ok(child_meta) = child.metadata() else {
                        continue;
                    };
                    if !child_meta.is_dir() {
                        continue;
                    }
                    if let Some(mut skill) = skill_folder_from_path(&child_path) {
                        skill.flock_slug = Some(flock_name.clone());
                        results.push(skill);
                    }
                }
            }
        }
    }
    results.sort_by(|left, right| left.slug.cmp(&right.slug));
    Ok(results)
}

pub fn copy_skill_folder(src: &Path, dst: &Path) -> Result<()> {
    if dst.exists() {
        fs::remove_dir_all(dst)
            .with_context(|| format!("failed to remove existing {}", dst.display()))?;
    }
    copy_dir_recursive(src, dst)
}

pub fn find_repo_skill_folders(repos_root: &Path) -> Result<Vec<RepoSkillFolder>> {
    let repos_root = repos_root
        .canonicalize()
        .unwrap_or_else(|_| repos_root.to_path_buf());
    let Ok(metadata) = fs::metadata(&repos_root) else {
        return Ok(Vec::new());
    };
    if !metadata.is_dir() {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();
    for entry in fs::read_dir(&repos_root)? {
        let entry = entry?;
        let repo_root = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if !metadata.is_dir() {
            continue;
        }

        let repo_name = repo_root
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_default();
        if repo_name.starts_with('.') {
            continue;
        }

        for skill in find_skill_folders(&repo_root)? {
            results.push(RepoSkillFolder {
                repo_name: repo_name.clone(),
                repo_root: repo_root.clone(),
                skill,
            });
        }
    }

    results.sort_by(|left, right| {
        left.repo_name
            .cmp(&right.repo_name)
            .then_with(|| left.skill.slug.cmp(&right.skill.slug))
    });
    Ok(results)
}

pub fn repo_git_commit(repo_root: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["-C"])
        .arg(repo_root)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    normalize_git_commit(stdout.trim()).or_else(|| clean_optional_string(Some(stdout)))
}

/// Read the SKILL.md content from a skill folder, trying both casing variants.
pub fn read_skill_md(skill_folder: &Path) -> Option<String> {
    for name in ["SKILL.md", "skill.md"] {
        let path = skill_folder.join(name);
        if let Ok(content) = fs::read_to_string(&path) {
            return Some(content);
        }
    }
    None
}

/// Extract the first line description from a SKILL.md content (after frontmatter).
pub fn extract_skill_description(content: &str) -> String {
    let body = if content.starts_with("---") {
        // Skip YAML frontmatter
        content
            .find("\n---")
            .and_then(|pos| content[pos + 4..].strip_prefix('\n'))
            .unwrap_or(content)
    } else {
        content
    };
    // Take first non-empty line as description
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let desc = trimmed.trim_start_matches("- ").trim_start_matches("* ");
        if !desc.is_empty() {
            return desc.chars().take(200).collect();
        }
    }
    String::new()
}

pub fn skill_folder_from_path(path: &Path) -> Option<SkillFolder> {
    let has_skill_md = ["SKILL.md", "skill.md"]
        .into_iter()
        .map(|name| path.join(name))
        .any(|candidate| candidate.is_file());
    if !has_skill_md {
        return None;
    }
    let base = path.file_name()?.to_string_lossy();
    if let Ok(files) = list_publishable_files(path) {
        if let Ok(Some(metadata)) = load_local_skill_metadata(&files) {
            return Some(SkillFolder {
                folder: path.to_path_buf(),
                slug: metadata.package.slug,
                display_name: metadata.package.name,
                flock_slug: None,
            });
        }
    }

    let slug = sanitize_slug(&base);
    if slug.is_empty() {
        return None;
    }

    Some(SkillFolder {
        folder: path.to_path_buf(),
        slug,
        display_name: title_case(&base),
        flock_slug: None,
    })
}

fn should_visit_entry(entry: &DirEntry) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    let name = entry.file_name().to_string_lossy();
    if name.starts_with('.') {
        return false;
    }
    !matches!(
        name.as_ref(),
        "node_modules" | ".git" | ".savhub" | "target"
    )
}

fn normalize_relative_path(root: &Path, path: &Path) -> Result<String> {
    let rel = path
        .strip_prefix(root)
        .with_context(|| format!("{} is not under {}", path.display(), root.display()))?;
    Ok(normalize_forward_slashes(rel))
}

fn normalize_forward_slashes(path: impl AsRef<Path>) -> String {
    path.as_ref()
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}

fn hash_bytes(value: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value);
    format!("{:x}", hasher.finalize())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            let name = src_path.file_name().unwrap_or_default().to_string_lossy();
            if name.starts_with('.') || name.as_ref() == "node_modules" || name.as_ref() == "target"
            {
                continue;
            }
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct SkillCatalogMeta {
    #[serde(default)]
    latest: Option<SkillCatalogMetaLatest>,
}

#[derive(Debug, Deserialize)]
struct SkillCatalogMetaLatest {
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    commit: Option<String>,
}

fn clean_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_git_commit(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }

    let candidate = trimmed
        .rsplit('/')
        .next()
        .unwrap_or(trimmed)
        .split(['?', '#'])
        .next()
        .unwrap_or(trimmed)
        .trim();

    if candidate.len() >= 7 && candidate.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Some(candidate.to_ascii_lowercase())
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Fetched skill metadata (shared between CLI and desktop)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct FetchedSkillMetadata {
    pub remote_id: Option<String>,
    pub remote_slug: Option<String>,
    pub repo_url: Option<String>,
    pub path: Option<String>,
    pub flock_slug: Option<String>,
    pub git_rev: Option<String>,
}

pub fn update_lockfile_with_metadata(
    workdir: &Path,
    _slug: &str,
    version: &str,
    metadata: &FetchedSkillMetadata,
) {
    let repo_url = match metadata.repo_url.as_deref() {
        Some(v) if !v.is_empty() => v,
        _ => return,
    };
    let path = match metadata.path.as_deref() {
        Some(v) if !v.is_empty() => v,
        _ => return,
    };
    let lock_path = workdir.join("skills.fetched.json");
    let mut lock = fs::read_to_string(&lock_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Lockfile>(&raw).ok())
        .unwrap_or_default();

    lock.repos.entry(repo_url.to_string()).or_default().insert(
        path.to_string(),
        LockEntry {
            version: version.to_string(),
            fetched_at: chrono::Utc::now().timestamp(),
            remote_id: metadata.remote_id.clone(),
            remote_slug: metadata.remote_slug.clone(),
            flock_slug: metadata.flock_slug.clone(),
            git_rev: metadata.git_rev.clone(),
        },
    );

    let _ = fs::write(
        &lock_path,
        serde_json::to_string_pretty(&lock).unwrap_or_default(),
    );
}

/// A flattened view of a lock entry with its repo_url and path.
#[derive(Debug, Clone)]
pub struct FlatLockEntry {
    pub repo_url: String,
    pub path: String,
    pub slug: String,
    pub entry: LockEntry,
}

/// Flatten the nested lockfile into a list of entries.
pub fn flatten_lockfile(lock: &Lockfile) -> Vec<FlatLockEntry> {
    let mut out = Vec::new();
    for (repo_url, paths) in &lock.repos {
        for (path, entry) in paths {
            let slug = entry
                .remote_slug
                .clone()
                .unwrap_or_else(|| path.rsplit('/').next().unwrap_or(path).to_string());
            out.push(FlatLockEntry {
                repo_url: repo_url.clone(),
                path: path.clone(),
                slug,
                entry: entry.clone(),
            });
        }
    }
    out
}

/// Read fetched skill versions from the lockfile. Returns slug → version.
pub fn read_fetched_skill_versions(workdir: &Path) -> std::collections::BTreeMap<String, String> {
    let lock = read_lockfile(workdir).unwrap_or_default();
    flatten_lockfile(&lock)
        .into_iter()
        .map(|e| (e.slug, e.entry.version))
        .collect()
}

/// Read the full lockfile entries as a flat slug → LockEntry map.
pub fn read_fetched_skill_entries(workdir: &Path) -> std::collections::BTreeMap<String, LockEntry> {
    let lock = read_lockfile(workdir).unwrap_or_default();
    flatten_lockfile(&lock)
        .into_iter()
        .map(|e| (e.slug, e.entry))
        .collect()
}

/// Count how many skills belong to a given flock_slug in the lockfile.
pub fn count_fetched_by_flock_slug(workdir: &Path, flock_slug: &str) -> usize {
    let lock = read_lockfile(workdir).unwrap_or_default();
    flatten_lockfile(&lock)
        .iter()
        .filter(|e| e.entry.flock_slug.as_deref() == Some(flock_slug))
        .count()
}

/// Collect the slugs of all fetched skills that belong to a given flock_slug.
pub fn fetched_slugs_by_flock_slug(workdir: &Path, flock_slug: &str) -> Vec<String> {
    let lock = read_lockfile(workdir).unwrap_or_default();
    flatten_lockfile(&lock)
        .into_iter()
        .filter(|e| e.entry.flock_slug.as_deref() == Some(flock_slug))
        .map(|e| e.slug)
        .collect()
}

/// Return the set of flock_slug values that have at least one fetched skill.
pub fn fetched_flock_slugs(workdir: &Path) -> std::collections::HashSet<String> {
    let lock = read_lockfile(workdir).unwrap_or_default();
    flatten_lockfile(&lock)
        .into_iter()
        .filter_map(|e| e.entry.flock_slug)
        .collect()
}

/// Remove a skill entry from the lockfile by slug.
pub fn prune_skill(workdir: &Path, slug: &str) -> Result<()> {
    let lock_path = workdir.join("skills.fetched.json");
    let mut lock = fs::read_to_string(&lock_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Lockfile>(&raw).ok())
        .unwrap_or_default();

    // Find and remove the entry matching slug
    for paths in lock.repos.values_mut() {
        paths.retain(|_path, entry| entry.remote_slug.as_deref() != Some(slug));
    }
    lock.repos.retain(|_, paths| !paths.is_empty());

    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &lock_path,
        serde_json::to_string_pretty(&lock).unwrap_or_default(),
    )?;
    Ok(())
}
