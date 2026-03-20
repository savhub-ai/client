use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::clients::{global_skills_dir, home_dir};
use crate::skills::{
    LockEntry, Lockfile, RepoSkillFolder, RepoSkillOrigin, SkillFolder, copy_skill_folder,
    find_repo_skill_folders, find_skill_folders, read_skill_version_info, repo_git_commit,
    skill_folder_from_path, write_repo_skill_origin,
};
use crate::utils::sanitize_slug;

/// A selector that matched for a project and the flocks/skills it contributed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSelectorMatch {
    #[serde(alias = "selector")]
    pub selector: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flocks: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repos: Vec<String>,
}

/// Selectors section in savhub.toml.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectSelectorsConfig {
    /// Selectors matched by `savhub apply` (auto-managed).
    #[serde(default)]
    pub matched: Vec<ProjectSelectorMatch>,
    /// User-manually-added selectors.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub manual_added: Vec<String>,
    /// User-manually-skipped selectors (never match).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub manual_skipped: Vec<String>,
}

/// A manually added skill, identified by its sign or path+slug.
///
/// Validation: a valid entry must have either a non-empty `sign`,
/// or both `path` and `slug` non-empty.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectAddedSkill {
    /// Skill sign: `{domain/owner/repo}/{source_path}`.
    /// e.g. `github.com/anthropics/skills/skills/claude-api`.
    /// If provided, `path` and `slug` can be omitted (derived at resolve time).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sign: Option<String>,
    /// Registry path that uniquely identifies the skill (legacy / explicit).
    #[serde(default)]
    pub path: String,
    /// Skill slug in the registry (e.g. `claude-api`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub slug: String,
    /// Local path relative to the project `skills/` directory.
    /// Only set when the local directory name differs from the slug
    /// (e.g. due to conflict resolution or flock-grouped layout).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default)]
    pub fetched_at: i64,
}

/// How skills are organized in the project `skills/` directory.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillLayout {
    /// Flat: `skills/{slug}/` (default)
    #[default]
    Flat,
    /// Grouped by flock: `skills/{flock_slug}/{skill_slug}/`
    Flock,
}

/// Skills section in savhub.toml.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectSkillsConfig {
    /// Directory layout for fetched skills.
    #[serde(default, skip_serializing_if = "is_default_layout")]
    pub layout: SkillLayout,
    /// User-manually-added skills.
    #[serde(default, alias = "added")]
    pub manual_added: Vec<ProjectAddedSkill>,
    /// Skill signs/slugs that should never be auto-fetched.
    #[serde(default, alias = "skipped")]
    pub manual_skipped: Vec<String>,
}

/// Flocks section in savhub.toml.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectFlocksConfig {
    /// Flocks contributed by matched selectors (auto-managed).
    #[serde(default)]
    pub matched: Vec<String>,
    /// User-manually-added flocks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub manual_added: Vec<String>,
    /// User-manually-skipped flocks (never fetch).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub manual_skipped: Vec<String>,
}

fn is_default_layout(layout: &SkillLayout) -> bool {
    *layout == SkillLayout::Flat
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectConfigFile {
    pub version: u8,
    #[serde(default)]
    pub selectors: ProjectSelectorsConfig,
    #[serde(default)]
    pub flocks: ProjectFlocksConfig,
    #[serde(default)]
    pub skills: ProjectSkillsConfig,
}

impl Default for ProjectConfigFile {
    fn default() -> Self {
        Self {
            version: 1,
            selectors: ProjectSelectorsConfig::default(),
            flocks: ProjectFlocksConfig::default(),
            skills: ProjectSkillsConfig::default(),
        }
    }
}

/// A locked skill entry, identified by its registry path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectLockedSkill {
    /// Skill sign: `{repo_sign}/{skill_path}` (e.g.
    /// `github.com/salvo-rs/salvo-skills/skills/salvo-auth`).
    pub sign: String,
    /// Skill version if available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Git commit hash of the fetched revision.
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "git_commit")]
    pub commit_hash: Option<String>,
}

impl ProjectLockedSkill {
    /// Derive the skill slug from the sign's last segment.
    pub fn slug(&self) -> &str {
        self.sign.rsplit('/').next().unwrap_or(&self.sign)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectLockFile {
    pub version: u8,
    pub skills: Vec<ProjectLockedSkill>,
}

impl Default for ProjectLockFile {
    fn default() -> Self {
        Self {
            version: 1,
            skills: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectSkillConflictChoice {
    Ask,
    UseRepo,
    KeepExisting,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSkillConflict {
    pub slug: String,
    pub repo_name: String,
    pub repo_skill_path: PathBuf,
    pub existing_skill_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnableProjectRepoSkillResult {
    Enabled {
        slug: String,
        overwritten: bool,
        version: Option<String>,
        git_commit: Option<String>,
    },
    KeptExisting {
        slug: String,
    },
    Conflict(ProjectSkillConflict),
}

// ---------------------------------------------------------------------------
// ProjectConfig I/O
// ---------------------------------------------------------------------------

fn project_config_path(workdir: &Path) -> PathBuf {
    let kdl = workdir.join("savhub.kdl");
    if kdl.exists() {
        return kdl;
    }
    let toml = workdir.join("savhub.toml");
    if toml.exists() {
        return toml;
    }
    // Default to KDL
    kdl
}

fn project_lock_path(workdir: &Path) -> PathBuf {
    workdir.join("savhub.lock")
}

pub fn project_skills_dir(workdir: &Path) -> PathBuf {
    workdir.join("skills")
}

/// Compute the local directory name for a skill given the layout.
///
/// - `Flat`: `{slug}` (or `{slug}-2`, `{slug}-3` on conflict)
/// - `Flock`: `{flock_slug}/{slug}` (or `{flock_slug}/{slug}-2` on conflict)
///
/// Returns `(local_path, renamed)` where `renamed` is true if a suffix was added.
pub fn compute_skill_local_path(
    skills_dir: &Path,
    slug: &str,
    flock_slug: Option<&str>,
    layout: SkillLayout,
) -> (String, bool) {
    let base_dir = match layout {
        SkillLayout::Flock => {
            let flock = flock_slug.unwrap_or("_default");
            format!("{flock}/{slug}")
        }
        SkillLayout::Flat => slug.to_string(),
    };

    // Try without suffix first
    if !skills_dir.join(&base_dir).exists() {
        return (base_dir, false);
    }

    // Conflict: append -{num}
    for i in 2..100 {
        let candidate = match layout {
            SkillLayout::Flock => {
                let flock = flock_slug.unwrap_or("_default");
                format!("{flock}/{slug}-{i}")
            }
            SkillLayout::Flat => format!("{slug}-{i}"),
        };
        if !skills_dir.join(&candidate).exists() {
            return (candidate, true);
        }
    }

    // Fallback (should never happen)
    (base_dir, false)
}

/// Read the skill layout from the project's savhub.toml.
pub fn read_project_skill_layout(workdir: &Path) -> SkillLayout {
    read_project_config(workdir)
        .map(|c| c.skills.layout)
        .unwrap_or_default()
}

pub fn repo_checkout_root() -> PathBuf {
    home_dir().join(".savhub").join("repos")
}

fn normalize_unique_slugs<I>(values: I) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    let mut normalized = Vec::new();
    for value in values {
        let slug = sanitize_slug(&value);
        if !slug.is_empty() && !normalized.contains(&slug) {
            normalized.push(slug);
        }
    }
    normalized
}

fn normalize_selector_matches(matches: &[ProjectSelectorMatch]) -> Vec<ProjectSelectorMatch> {
    let mut normalized = Vec::new();
    for matched in matches {
        let selector = matched.selector.trim().to_string();
        if selector.is_empty() {
            continue;
        }
        let flocks = normalize_unique_slugs(matched.flocks.clone());
        let skills = normalize_unique_slugs(matched.skills.clone());
        let repos = normalize_unique_slugs(matched.repos.clone());
        let duplicate = normalized
            .iter()
            .any(|existing: &ProjectSelectorMatch| existing.selector == selector);
        if !duplicate {
            normalized.push(ProjectSelectorMatch {
                selector,
                flocks,
                skills,
                repos,
            });
        }
    }
    normalized
}

fn normalize_added_skills(skills: &[ProjectAddedSkill]) -> Vec<ProjectAddedSkill> {
    let mut normalized = Vec::new();
    for skill in skills {
        let sign = skill
            .sign
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());
        let path = sanitize_slug(&skill.path);
        let slug = skill.slug.trim().to_string();

        // Validation: must have either a sign, or both path and slug.
        let has_sign = sign.is_some();
        let has_path_slug = !path.is_empty() && !slug.is_empty();
        if !has_sign && !has_path_slug {
            continue;
        }

        // Use sign's last segment as slug fallback, path fallback.
        let effective_slug = if !slug.is_empty() {
            slug
        } else if let Some(s) = sign {
            s.rsplit('/').next().unwrap_or(s).to_string()
        } else {
            continue;
        };
        let effective_path = if !path.is_empty() {
            path
        } else {
            effective_slug.clone()
        };

        if let Some(existing) = normalized
            .iter_mut()
            .find(|existing: &&mut ProjectAddedSkill| existing.path == effective_path)
        {
            let existing_empty = existing
                .version
                .as_deref()
                .map(|v| v.trim().is_empty())
                .unwrap_or(true);
            let existing_latest = existing.version.as_deref() == Some("latest");
            if existing_empty || existing_latest {
                existing.version = skill.version.clone();
            }
            existing.fetched_at = existing.fetched_at.max(skill.fetched_at);
            continue;
        }
        normalized.push(ProjectAddedSkill {
            sign: sign.map(String::from),
            path: effective_path,
            slug: effective_slug,
            local: skill.local.clone(),
            version: skill.version.clone(),
            fetched_at: skill.fetched_at,
        });
    }
    normalized.sort_by(|left, right| left.path.cmp(&right.path));
    normalized
}

fn normalize_project_lock_skills(skills: &[ProjectLockedSkill]) -> Vec<ProjectLockedSkill> {
    let mut normalized = Vec::new();
    for skill in skills {
        let sign = skill.sign.trim().to_string();
        if sign.is_empty()
            || normalized
                .iter()
                .any(|existing: &ProjectLockedSkill| existing.sign == sign)
        {
            continue;
        }
        normalized.push(ProjectLockedSkill {
            sign,
            version: skill
                .version
                .as_ref()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty()),
            commit_hash: skill
                .commit_hash
                .as_ref()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty()),
        });
    }
    normalized.sort_by(|left, right| left.sign.cmp(&right.sign));
    normalized
}

fn lockfile_to_project_added_skills(lockfile: &Lockfile) -> Vec<ProjectAddedSkill> {
    lockfile
        .skills
        .iter()
        .map(|(slug, entry)| ProjectAddedSkill {
            sign: None,
            path: slug.clone(),
            slug: slug.clone(),
            local: None,
            version: Some(entry.version.clone()),
            fetched_at: entry.fetched_at,
        })
        .collect()
}

fn project_added_skills_to_lockfile(skills: &[ProjectAddedSkill]) -> Lockfile {
    let mut lockfile = Lockfile::default();
    for skill in normalize_added_skills(skills) {
        lockfile.skills.insert(
            skill.path,
            LockEntry {
                version: skill.version.unwrap_or_else(|| "latest".to_string()),
                fetched_at: skill.fetched_at,
            },
        );
    }
    lockfile
}

pub fn read_project_config(workdir: &Path) -> Result<ProjectConfigFile> {
    let path = project_config_path(workdir);
    if let Ok(raw) = fs::read_to_string(&path) {
        let mut config: ProjectConfigFile = if crate::kdl_support::is_kdl_path(&path) {
            crate::kdl_support::parse_kdl(&raw)
                .map_err(|e| anyhow::anyhow!(e))
                .with_context(|| format!("invalid {}", path.display()))?
        } else {
            toml::from_str(&raw).with_context(|| format!("invalid {}", path.display()))?
        };
        config.selectors.matched = normalize_selector_matches(&config.selectors.matched);
        config.skills.manual_added = normalize_added_skills(&config.skills.manual_added);
        return Ok(config);
    }
    Ok(ProjectConfigFile::default())
}

pub fn write_project_config(workdir: &Path, config: &ProjectConfigFile) -> Result<()> {
    write_project_config_inner(workdir, config, false)
}

/// Write project config, always creating the file even if all sections are empty.
pub fn write_project_config_force(workdir: &Path, config: &ProjectConfigFile) -> Result<()> {
    write_project_config_inner(workdir, config, true)
}

fn write_project_config_inner(
    workdir: &Path,
    config: &ProjectConfigFile,
    force: bool,
) -> Result<()> {
    let path = project_config_path(workdir);
    let mut normalized = config.clone();
    normalized.version = 1;
    normalized.selectors.matched = normalize_selector_matches(&normalized.selectors.matched);
    normalized.skills.manual_added = normalize_added_skills(&normalized.skills.manual_added);

    if !force
        && normalized.selectors.matched.is_empty()
        && normalized.selectors.manual_added.is_empty()
        && normalized.selectors.manual_skipped.is_empty()
        && normalized.skills.manual_added.is_empty()
        && normalized.skills.manual_skipped.is_empty()
        && normalized.flocks.matched.is_empty()
        && normalized.flocks.manual_added.is_empty()
        && normalized.flocks.manual_skipped.is_empty()
    {
        if path.exists() {
            fs::remove_file(path)?;
        }
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let payload = if crate::kdl_support::is_kdl_path(&path) {
        crate::kdl_support::to_kdl_string(&normalized).map_err(|e| anyhow::anyhow!(e))?
    } else {
        toml::to_string_pretty(&normalized)?
    };
    fs::write(path, format!("{payload}\n"))?;
    Ok(())
}

pub fn read_project_lockfile(workdir: &Path) -> Result<ProjectLockFile> {
    let path = project_lock_path(workdir);
    let Ok(raw) = fs::read_to_string(&path) else {
        return Ok(ProjectLockFile::default());
    };
    let config_path = project_config_path(workdir);
    let mut lockfile: ProjectLockFile = if crate::kdl_support::is_kdl_path(&config_path) {
        crate::kdl_support::parse_kdl(&raw)
            .map_err(|e| anyhow::anyhow!(e))
            .with_context(|| format!("invalid {}", path.display()))?
    } else {
        toml::from_str(&raw).with_context(|| format!("invalid {}", path.display()))?
    };
    lockfile.version = 1;
    lockfile.skills = normalize_project_lock_skills(&lockfile.skills);
    Ok(lockfile)
}

pub fn write_project_lockfile(workdir: &Path, lockfile: &ProjectLockFile) -> Result<()> {
    write_project_lockfile_inner(workdir, lockfile, false)
}

/// Write lockfile, always creating the file even if skills list is empty.
pub fn write_project_lockfile_force(workdir: &Path, lockfile: &ProjectLockFile) -> Result<()> {
    write_project_lockfile_inner(workdir, lockfile, true)
}

fn write_project_lockfile_inner(
    workdir: &Path,
    lockfile: &ProjectLockFile,
    force: bool,
) -> Result<()> {
    let path = project_lock_path(workdir);
    let mut normalized = lockfile.clone();
    normalized.version = 1;
    normalized.skills = normalize_project_lock_skills(&normalized.skills);

    if !force && normalized.skills.is_empty() {
        if path.exists() {
            fs::remove_file(path)?;
        }
        return Ok(());
    }

    let config_path = project_config_path(workdir);
    let payload = if crate::kdl_support::is_kdl_path(&config_path) {
        crate::kdl_support::to_kdl_string(&normalized).map_err(|e| anyhow::anyhow!(e))?
    } else {
        toml::to_string_pretty(&normalized)?
    };
    fs::write(path, format!("{payload}\n"))?;
    Ok(())
}

pub fn read_project_selector_matches(workdir: &Path) -> Result<Vec<ProjectSelectorMatch>> {
    Ok(read_project_config(workdir)?.selectors.matched)
}

pub fn read_project_added_skills(workdir: &Path) -> Result<Lockfile> {
    Ok(project_added_skills_to_lockfile(
        &read_project_config(workdir)?.skills.manual_added,
    ))
}

fn upsert_project_added_skill(workdir: &Path, skill: ProjectAddedSkill) -> Result<()> {
    let mut config = read_project_config(workdir)?;
    if let Some(existing) = config
        .skills
        .manual_added
        .iter_mut()
        .find(|existing| existing.path == skill.path)
    {
        existing.version = skill.version;
        existing.fetched_at = skill.fetched_at;
    } else {
        config.skills.manual_added.push(skill);
    }
    write_project_config(workdir, &config)?;
    sync_project_lock(workdir)
}

fn remove_project_added_skill(workdir: &Path, slug: &str) -> Result<()> {
    let mut config = read_project_config(workdir)?;
    config
        .skills
        .manual_added
        .retain(|skill| skill.path != slug);
    write_project_config(workdir, &config)?;
    sync_project_lock(workdir)
}

pub fn write_project_added_skills(workdir: &Path, lockfile: &Lockfile) -> Result<()> {
    let mut config = read_project_config(workdir)?;
    config.skills.manual_added = lockfile_to_project_added_skills(lockfile);
    write_project_config(workdir, &config)?;
    sync_project_lock(workdir)
}

fn find_repo_skill(repo_name: &str, slug: &str) -> Result<RepoSkillFolder> {
    let repo_name = repo_name.trim();
    let slug = sanitize_slug(slug);
    if repo_name.is_empty() || slug.is_empty() {
        bail!("invalid repo skill selection");
    }

    list_repo_skills()?
        .into_iter()
        .find(|candidate| {
            candidate.repo_name.eq_ignore_ascii_case(repo_name) && candidate.skill.slug == slug
        })
        .with_context(|| format!("skill '{slug}' not found in repo '{repo_name}'"))
}

pub fn enable_repo_skill_in_project(
    workdir: &Path,
    repo_name: &str,
    slug: &str,
    conflict_choice: ProjectSkillConflictChoice,
    _sources: ResolvedSkillSources,
) -> Result<EnableProjectRepoSkillResult> {
    let repo_skill = find_repo_skill(repo_name, slug)?;
    let slug = repo_skill.skill.slug.clone();
    let target = project_skills_dir(workdir).join(&slug);
    let overwritten = target.exists();

    if overwritten {
        let conflict = ProjectSkillConflict {
            slug: slug.clone(),
            repo_name: repo_skill.repo_name.clone(),
            repo_skill_path: repo_skill.skill.folder.clone(),
            existing_skill_path: target.clone(),
        };
        match conflict_choice {
            ProjectSkillConflictChoice::Ask => {
                return Ok(EnableProjectRepoSkillResult::Conflict(conflict));
            }
            ProjectSkillConflictChoice::KeepExisting => {
                return Ok(EnableProjectRepoSkillResult::KeptExisting { slug });
            }
            ProjectSkillConflictChoice::UseRepo => {}
        }
    }

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    copy_skill_folder(&repo_skill.skill.folder, &target)?;

    let mut version_info = read_skill_version_info(&repo_skill.skill.folder).unwrap_or_default();
    if version_info.git_commit.is_none() {
        version_info.git_commit = repo_git_commit(&repo_skill.repo_root);
    }

    let fetched_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    write_repo_skill_origin(
        &target,
        &RepoSkillOrigin {
            version: 1,
            repo: repo_skill.repo_name.clone(),
            repo_sign: repo_skill.repo_root.display().to_string(),
            repo_commit: version_info.git_commit.clone(),
            slug: slug.clone(),
            skill_version: version_info.version.clone(),
            fetched_at,
        },
    )?;

    upsert_project_added_skill(
        workdir,
        ProjectAddedSkill {
            sign: None,
            path: slug.clone(),
            slug: slug.clone(),
            local: None,
            version: version_info.version.clone(),
            fetched_at,
        },
    )?;

    Ok(EnableProjectRepoSkillResult::Enabled {
        slug,
        overwritten,
        version: version_info.version,
        git_commit: version_info.git_commit,
    })
}

pub fn disable_project_skill(workdir: &Path, slug: &str) -> Result<bool> {
    let slug = sanitize_slug(slug);
    if slug.is_empty() {
        bail!("invalid skill slug");
    }

    let mut removed_any = false;
    for skills_dir in project_skill_search_dirs(workdir) {
        let target = skills_dir.join(&slug);
        if target.exists() {
            fs::remove_dir_all(&target)
                .with_context(|| format!("failed to remove {}", target.display()))?;
            removed_any = true;
        }
    }

    let config = read_project_config(workdir)?;
    let had_manual_entry = config
        .skills
        .manual_added
        .iter()
        .any(|skill| skill.path == slug);
    if had_manual_entry {
        remove_project_added_skill(workdir, &slug)?;
        removed_any = true;
    } else if removed_any {
        sync_project_lock(workdir)?;
    }

    Ok(removed_any)
}

// ---------------------------------------------------------------------------
// Skill resolution
// ---------------------------------------------------------------------------

/// Information about a resolved skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSkill {
    pub slug: String,
    pub display_name: String,
    pub folder: PathBuf,
}

/// Provenance for an enabled skill in a project.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ResolvedSkillSources {
    pub selectors: Vec<String>,
    pub flocks: Vec<String>,
    pub manual: bool,
}

/// A resolved skill with source metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedProjectSkill {
    pub slug: String,
    pub display_name: String,
    pub folder: PathBuf,
    pub sources: ResolvedSkillSources,
}

fn add_unique(items: &mut Vec<String>, value: &str) {
    if !value.is_empty() && !items.iter().any(|item| item == value) {
        items.push(value.to_string());
    }
}

pub fn list_repo_skills() -> Result<Vec<RepoSkillFolder>> {
    find_repo_skill_folders(&repo_checkout_root())
}

fn project_skill_search_dirs(workdir: &Path) -> Vec<PathBuf> {
    let primary = project_skills_dir(workdir);
    vec![primary]
}

fn collect_skill_folders(workdir: &Path) -> Vec<SkillFolder> {
    let mut all_folders: Vec<SkillFolder> = Vec::new();

    for project_dir in project_skill_search_dirs(workdir) {
        if project_dir.is_dir() {
            all_folders.extend(find_skill_folders(&project_dir).unwrap_or_default());
        }
    }

    let global_dir = global_skills_dir();
    if global_dir.is_dir() {
        for folder in find_skill_folders(&global_dir).unwrap_or_default() {
            if all_folders
                .iter()
                .any(|existing| existing.slug == folder.slug)
            {
                continue;
            }
            all_folders.push(folder);
        }
    }

    // 3. Repo-fetched skills (from installed_skills.json)
    if let Ok(fetched) = crate::registry::read_fetched_skills_file() {
        for entry in fetched {
            if let Some(path) = crate::registry::fetched_skill_local_path(&entry) {
                if path.is_dir() {
                    if let Some(skill) = skill_folder_from_path(&path) {
                        if !all_folders.iter().any(|e| e.slug == skill.slug) {
                            all_folders.push(skill);
                        }
                    }
                }
            }
        }
    }

    all_folders
}

fn build_project_lockfile(resolved: &[ResolvedProjectSkill]) -> ProjectLockFile {
    let skills = resolved
        .iter()
        .map(|skill| {
            let version_info = read_skill_version_info(&skill.folder).unwrap_or_default();
            ProjectLockedSkill {
                sign: skill.slug.clone(),
                version: version_info.version,
                commit_hash: version_info.git_commit,
            }
        })
        .collect::<Vec<_>>();

    ProjectLockFile { version: 1, skills }
}

fn resolve_project_skills_internal(workdir: &Path) -> Result<Vec<ResolvedProjectSkill>> {
    let config = read_project_config(workdir)?;
    let mut sources = BTreeMap::<String, ResolvedSkillSources>::new();

    // Expand skills from matched selectors
    for matched in &config.selectors.matched {
        for skill_slug in &matched.skills {
            let entry = sources.entry(skill_slug.clone()).or_default();
            add_unique(&mut entry.selectors, &matched.selector);
        }
        // Expand flocks from each matched selector
        for flock_slug in &matched.flocks {
            if let Ok(skill_slugs) = crate::registry::list_flock_skill_slugs(flock_slug) {
                for skill_slug in skill_slugs {
                    let entry = sources.entry(skill_slug).or_default();
                    add_unique(&mut entry.flocks, flock_slug);
                    add_unique(&mut entry.selectors, &matched.selector);
                }
            }
        }
        // Expand repos: look up all flocks in each repo, then expand those flocks
        for repo_sign in &matched.repos {
            if let Ok(repo_flocks) = crate::registry::list_repo_flock_signs(repo_sign) {
                for flock_slug in &repo_flocks {
                    if let Ok(skill_slugs) = crate::registry::list_flock_skill_slugs(flock_slug) {
                        for skill_slug in skill_slugs {
                            let entry = sources.entry(skill_slug).or_default();
                            add_unique(&mut entry.flocks, flock_slug);
                            add_unique(&mut entry.selectors, &matched.selector);
                        }
                    }
                }
            }
        }
    }

    for skill in &config.skills.manual_added {
        let entry = sources.entry(skill.path.clone()).or_default();
        entry.manual = true;
    }

    // Expand flocks: matched + manual_added, filter out manual_skipped
    let mut all_flocks = config.flocks.matched.clone();
    for slug in &config.flocks.manual_added {
        if !all_flocks.contains(slug) {
            all_flocks.push(slug.clone());
        }
    }
    all_flocks.retain(|s| !config.flocks.manual_skipped.contains(s));
    for flock_slug in &all_flocks {
        if let Ok(skill_slugs) = crate::registry::list_flock_skill_slugs(flock_slug) {
            for skill_slug in skill_slugs {
                let entry = sources.entry(skill_slug).or_default();
                add_unique(&mut entry.flocks, flock_slug);
            }
        }
    }

    let all_folders = collect_skill_folders(workdir);
    let mut resolved = Vec::new();
    for (slug, source) in sources {
        if let Some(folder) = all_folders.iter().find(|candidate| candidate.slug == slug) {
            resolved.push(ResolvedProjectSkill {
                slug: folder.slug.clone(),
                display_name: folder.display_name.clone(),
                folder: folder.folder.clone(),
                sources: source,
            });
        }
    }

    resolved.sort_by(|left, right| left.slug.cmp(&right.slug));
    Ok(resolved)
}

pub fn sync_project_lock(workdir: &Path) -> Result<()> {
    let resolved = resolve_project_skills_internal(workdir)?;
    write_project_lockfile(workdir, &build_project_lockfile(&resolved))
}

pub fn resolve_project_skills_with_sources(workdir: &Path) -> Result<Vec<ResolvedProjectSkill>> {
    let resolved = resolve_project_skills_internal(workdir)?;
    let _ = write_project_lockfile(workdir, &build_project_lockfile(&resolved));
    Ok(resolved)
}

/// Resolve the list of skills for a project directory.
///
/// Priority:
/// 1. Selector-matched skills and flocks
/// 2. Project-local manual skills from `savhub.toml`
///
/// Skill folders are looked up in the global skills directory.
pub fn resolve_skills_for_project(workdir: &Path) -> Result<Vec<ResolvedSkill>> {
    Ok(resolve_project_skills_with_sources(workdir)?
        .into_iter()
        .map(|skill| ResolvedSkill {
            slug: skill.slug,
            display_name: skill.display_name,
            folder: skill.folder,
        })
        .collect())
}
