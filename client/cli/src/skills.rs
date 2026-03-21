use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use savhub_shared::{BundleMetadata, PublishBundleFile, ResourceKind, load_bundle_metadata};
use sha2::{Digest, Sha256};
use walkdir::{DirEntry, WalkDir};
use zip::ZipArchive;

pub use savhub_shared::{LockEntry, Lockfile};

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

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct SkillFolder {
    pub folder: PathBuf,
    pub slug: String,
    pub display_name: String,
}

pub fn read_lockfile(workdir: &Path) -> Result<Lockfile> {
    for path in lockfile_paths(workdir) {
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(lockfile) = serde_json::from_str::<Lockfile>(&raw) else {
            continue;
        };
        return Ok(lockfile);
    }
    Ok(Lockfile::default())
}

pub fn write_lockfile(workdir: &Path, lockfile: &Lockfile) -> Result<()> {
    let path = workdir.join(".savhub").join("lock.json");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(lockfile)?),
    )?;
    Ok(())
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

#[allow(dead_code)]
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

#[allow(dead_code)]
pub fn load_local_skill_metadata(files: &[LocalSkillFile]) -> Result<Option<BundleMetadata>> {
    let publish_files = files
        .iter()
        .map(|file| PublishBundleFile {
            path: file.path.clone(),
            content: file.content.clone(),
        })
        .collect::<Vec<_>>();
    load_bundle_metadata(&publish_files, ResourceKind::Skill)
        .map_err(|error| anyhow::anyhow!(error))
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

#[allow(dead_code)]
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
        }
    }
    results.sort_by(|left, right| left.slug.cmp(&right.slug));
    Ok(results)
}

#[allow(dead_code)]
pub fn sanitize_slug(value: &str) -> String {
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

#[allow(dead_code)]
pub fn title_case(value: &str) -> String {
    value
        .trim()
        .replace(['-', '_'], " ")
        .split_whitespace()
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => {
                    let mut chunk = first.to_uppercase().to_string();
                    chunk.push_str(chars.as_str());
                    chunk
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[allow(dead_code)]
fn skill_folder_from_path(path: &Path) -> Option<SkillFolder> {
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

fn lockfile_paths(workdir: &Path) -> [PathBuf; 1] {
    [workdir.join(".savhub").join("lock.json")]
}

fn hash_bytes(value: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value);
    format!("{:x}", hasher.finalize())
}
