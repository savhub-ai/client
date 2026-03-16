use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::clients::{global_skills_dir, home_dir};
use crate::config::get_config_dir;
use crate::skills::{
    LockEntry, Lockfile, RepoSkillFolder, RepoSkillOrigin, SkillFolder, copy_skill_folder,
    find_repo_skill_folders, find_skill_folders, read_lockfile, read_skill_version_info,
    repo_git_commit, sanitize_slug, skill_folder_from_path, write_repo_skill_origin,
};

/// A named combination of skills.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetConfig {
    pub sign: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Skill signs included in this preset.
    pub skills: Vec<String>,
    /// Flock signs included in this preset.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flocks: Vec<String>,
}

/// Persistent store for all preset definitions.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PresetsStore {
    pub version: u8,
    #[serde(alias = "profiles")]
    pub presets: BTreeMap<String, PresetConfig>,
}

/// Per-project preset binding.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectPreset {
    #[serde(alias = "profile")]
    pub preset: String,
}

/// A selector that matched for a project and the presets/flocks it contributed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSelectorMatch {
    #[serde(alias = "selector")]
    pub selector: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub presets: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flocks: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<String>,
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
    pub installed_at: i64,
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
    /// Directory layout for installed skills.
    #[serde(default, skip_serializing_if = "is_default_layout")]
    pub layout: SkillLayout,
    /// User-manually-added skills.
    #[serde(default, alias = "added")]
    pub manual_added: Vec<ProjectAddedSkill>,
    /// Skill signs/slugs that should never be auto-installed.
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
    /// User-manually-skipped flocks (never install).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub manual_skipped: Vec<String>,
}

/// Presets section in savhub.toml.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectPresetsConfig {
    /// Presets contributed by matched selectors (auto-managed).
    #[serde(default)]
    pub matched: Vec<String>,
    /// User-manually-added presets.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub manual_added: Vec<String>,
    /// User-manually-skipped presets (never enable).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub manual_skipped: Vec<String>,
}

fn is_default_layout(layout: &SkillLayout) -> bool {
    *layout == SkillLayout::Flat
}

#[derive(Debug, Clone, Default)]
struct ProjectBindings {
    presets: ProjectPresetsConfig,
    selectors: ProjectSelectorsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectConfigFile {
    pub version: u8,
    #[serde(default)]
    pub selectors: ProjectSelectorsConfig,
    #[serde(default)]
    pub presets: ProjectPresetsConfig,
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
            presets: ProjectPresetsConfig::default(),
            flocks: ProjectFlocksConfig::default(),
            skills: ProjectSkillsConfig::default(),
        }
    }
}

/// A locked skill entry, identified by its registry path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectLockedSkill {
    /// Repo path relative to registry data dir (e.g. `github.com/salvo-rs/salvo-skills`).
    pub repo: String,
    /// Skill path relative to git repo root (e.g. `skills/salvo-auth`).
    pub path: String,
    /// Skill version if available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Git commit hash of the installed revision.
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "git_commit")]
    pub commit_hash: Option<String>,
}

impl ProjectLockedSkill {
    /// Derive the skill slug from the path's last segment.
    pub fn slug(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
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
// PresetsStore I/O
// ---------------------------------------------------------------------------

fn presets_path() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("profiles.json"))
}

pub fn read_presets_store() -> Result<PresetsStore> {
    let path = presets_path()?;
    let Ok(raw) = fs::read_to_string(&path) else {
        return Ok(PresetsStore {
            version: 1,
            ..Default::default()
        });
    };
    serde_json::from_str(&raw).with_context(|| format!("invalid profiles at {}", path.display()))
}

pub fn write_presets_store(store: &PresetsStore) -> Result<()> {
    let path = presets_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let payload = serde_json::to_string_pretty(store)?;
    fs::write(&path, format!("{payload}\n"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// ProjectPreset I/O
// ---------------------------------------------------------------------------

fn project_config_path(workdir: &Path) -> PathBuf {
    workdir.join("savhub.toml")
}

fn project_lock_path(workdir: &Path) -> PathBuf {
    workdir.join("savhub.lock")
}

pub fn project_skills_dir(workdir: &Path) -> PathBuf {
    workdir.join("skills")
}

pub fn legacy_project_skills_dir(workdir: &Path) -> PathBuf {
    workdir.join(".savhub").join("skills")
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

fn legacy_project_preset_path(workdir: &Path) -> PathBuf {
    workdir.join(".savhub").join("profile.json")
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
        let presets = normalize_unique_slugs(matched.presets.clone());
        let flocks = normalize_unique_slugs(matched.flocks.clone());
        let skills = normalize_unique_slugs(matched.skills.clone());
        let duplicate = normalized.iter().any(|existing: &ProjectSelectorMatch| {
            existing.selector == selector && existing.presets == presets
        });
        if !duplicate {
            normalized.push(ProjectSelectorMatch { selector, presets, flocks, skills });
        }
    }
    normalized
}

fn normalize_added_skills(skills: &[ProjectAddedSkill]) -> Vec<ProjectAddedSkill> {
    let mut normalized = Vec::new();
    for skill in skills {
        let sign = skill.sign.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty());
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
            let existing_empty = existing.version.as_deref().map(|v| v.trim().is_empty()).unwrap_or(true);
            let existing_latest = existing.version.as_deref() == Some("latest");
            if existing_empty || existing_latest {
                existing.version = skill.version.clone();
            }
            existing.installed_at = existing.installed_at.max(skill.installed_at);
            continue;
        }
        normalized.push(ProjectAddedSkill {
            sign: sign.map(String::from),
            path: effective_path,
            slug: effective_slug,
            local: skill.local.clone(),
            version: skill.version.clone(),
            installed_at: skill.installed_at,
        });
    }
    normalized.sort_by(|left, right| left.path.cmp(&right.path));
    normalized
}

fn normalize_project_lock_skills(skills: &[ProjectLockedSkill]) -> Vec<ProjectLockedSkill> {
    let mut normalized = Vec::new();
    for skill in skills {
        let path = skill.path.trim().to_string();
        if path.is_empty()
            || normalized
                .iter()
                .any(|existing: &ProjectLockedSkill| existing.path == path)
        {
            continue;
        }
        normalized.push(ProjectLockedSkill {
            repo: skill.repo.trim().to_string(),
            path,
            version: skill
                .version
                .as_ref()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            commit_hash: skill
                .commit_hash
                .as_ref()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
        });
    }
    normalized.sort_by(|left, right| left.path.cmp(&right.path));
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
            installed_at: entry.installed_at,
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
                installed_at: skill.installed_at,
            },
        );
    }
    lockfile
}

fn read_legacy_project_bindings(workdir: &Path) -> Result<ProjectBindings> {
    let path = legacy_project_preset_path(workdir);
    let Ok(raw) = fs::read_to_string(&path) else {
        return Ok(ProjectBindings::default());
    };
    let payload: Value = serde_json::from_str(&raw)
        .with_context(|| format!("invalid project preset at {}", path.display()))?;

    let presets = if let Some(name) = payload.get("profile").and_then(Value::as_str) {
        normalize_unique_slugs([name.to_string()])
    } else if let Some(items) = payload.get("presets").and_then(Value::as_array) {
        normalize_unique_slugs(
            items
                .iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect::<Vec<_>>(),
        )
    } else {
        Vec::new()
    };

    let matched_selectors = payload
        .get("matchedSelectors")
        .and_then(Value::as_array)
        .map(|items| {
            normalize_selector_matches(
                &items
                    .iter()
                    .map(|item| ProjectSelectorMatch {
                        selector: item
                            .get("selector")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        presets: item
                            .get("presets")
                            .and_then(Value::as_array)
                            .map(|presets| {
                                presets
                                    .iter()
                                    .filter_map(Value::as_str)
                                    .map(String::from)
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default(),
                        flocks: Vec::new(),
                        skills: Vec::new(),
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap_or_default();

    Ok(ProjectBindings {
        presets: ProjectPresetsConfig {
            matched: presets,
            ..Default::default()
        },
        selectors: ProjectSelectorsConfig {
            matched: matched_selectors,
            manual_added: Vec::new(),
            manual_skipped: Vec::new(),
        },
    })
}

pub fn read_project_config(workdir: &Path) -> Result<ProjectConfigFile> {
    let path = project_config_path(workdir);
    if let Ok(raw) = fs::read_to_string(&path) {
        let mut config: ProjectConfigFile =
            toml::from_str(&raw).with_context(|| format!("invalid {}", path.display()))?;
        config.presets.matched = normalize_unique_slugs(config.presets.matched);
        config.presets.manual_added = normalize_unique_slugs(config.presets.manual_added);
        config.selectors.matched = normalize_selector_matches(&config.selectors.matched);
        config.skills.manual_added = normalize_added_skills(&config.skills.manual_added);
        return Ok(config);
    }

    let bindings = read_legacy_project_bindings(workdir)?;
    let manual_skills = lockfile_to_project_added_skills(&read_lockfile(workdir)?);
    Ok(ProjectConfigFile {
        presets: bindings.presets,
        selectors: bindings.selectors,
        skills: ProjectSkillsConfig {
            layout: SkillLayout::default(),
            manual_added: manual_skills,
            manual_skipped: Vec::new(),
        },
        ..ProjectConfigFile::default()
    })
}

fn remove_legacy_project_files(workdir: &Path) {
    let _ = fs::remove_file(legacy_project_preset_path(workdir));
    let _ = fs::remove_file(workdir.join(".savhub").join("lock.json"));
}

pub fn write_project_config(workdir: &Path, config: &ProjectConfigFile) -> Result<()> {
    write_project_config_inner(workdir, config, false)
}

/// Write project config, always creating the file even if all sections are empty.
pub fn write_project_config_force(workdir: &Path, config: &ProjectConfigFile) -> Result<()> {
    write_project_config_inner(workdir, config, true)
}

fn write_project_config_inner(workdir: &Path, config: &ProjectConfigFile, force: bool) -> Result<()> {
    let path = project_config_path(workdir);
    let mut normalized = config.clone();
    normalized.version = 1;
    normalized.presets.matched = normalize_unique_slugs(normalized.presets.matched);
    normalized.presets.manual_added = normalize_unique_slugs(normalized.presets.manual_added);
    normalized.selectors.matched = normalize_selector_matches(&normalized.selectors.matched);
    normalized.skills.manual_added = normalize_added_skills(&normalized.skills.manual_added);

    if !force
        && normalized.presets.matched.is_empty()
        && normalized.presets.manual_added.is_empty()
        && normalized.presets.manual_skipped.is_empty()
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
        remove_legacy_project_files(workdir);
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let payload = toml::to_string_pretty(&normalized)?;
    fs::write(path, format!("{payload}\n"))?;
    remove_legacy_project_files(workdir);
    Ok(())
}

pub fn read_project_lockfile(workdir: &Path) -> Result<ProjectLockFile> {
    let path = project_lock_path(workdir);
    let Ok(raw) = fs::read_to_string(&path) else {
        return Ok(ProjectLockFile::default());
    };
    let mut lockfile: ProjectLockFile =
        toml::from_str(&raw).with_context(|| format!("invalid {}", path.display()))?;
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

fn write_project_lockfile_inner(workdir: &Path, lockfile: &ProjectLockFile, force: bool) -> Result<()> {
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

    let payload = toml::to_string_pretty(&normalized)?;
    fs::write(path, format!("{payload}\n"))?;
    Ok(())
}

fn read_project_bindings(workdir: &Path) -> Result<ProjectBindings> {
    let config = read_project_config(workdir)?;
    Ok(ProjectBindings {
        presets: config.presets,
        selectors: config.selectors,
    })
}

fn write_project_bindings(workdir: &Path, bindings: &ProjectBindings) -> Result<()> {
    let mut config = read_project_config(workdir)?;
    config.presets = bindings.presets.clone();
    config.selectors.matched = bindings.selectors.matched.clone();
    write_project_config(workdir, &config)
}

pub fn read_project_preset(workdir: &Path) -> Result<Option<ProjectPreset>> {
    let presets = read_project_presets(workdir)?;
    if let Some(preset) = presets.into_iter().next() {
        return Ok(Some(ProjectPreset { preset }));
    }

    Ok(None)
}

pub fn read_project_presets(workdir: &Path) -> Result<Vec<String>> {
    let presets_config = read_project_bindings(workdir)?.presets;
    let mut result = presets_config.matched;
    for slug in presets_config.manual_added {
        if !result.contains(&slug) {
            result.push(slug);
        }
    }
    result.retain(|s| !presets_config.manual_skipped.contains(s));
    Ok(result)
}

pub fn read_project_selector_matches(workdir: &Path) -> Result<Vec<ProjectSelectorMatch>> {
    Ok(read_project_bindings(workdir)?.selectors.matched)
}

pub fn read_project_added_skills(workdir: &Path) -> Result<Lockfile> {
    Ok(project_added_skills_to_lockfile(
        &read_project_config(workdir)?.skills.manual_added,
    ))
}

fn upsert_project_added_skill(workdir: &Path, skill: ProjectAddedSkill) -> Result<()> {
    let mut config = read_project_config(workdir)?;
    if let Some(existing) = config
        .skills.manual_added
        .iter_mut()
        .find(|existing| existing.path == skill.path)
    {
        existing.version = skill.version;
        existing.installed_at = skill.installed_at;
    } else {
        config.skills.manual_added.push(skill);
    }
    write_project_config(workdir, &config)?;
    sync_project_lock(workdir)
}

fn remove_project_added_skill(workdir: &Path, slug: &str) -> Result<()> {
    let mut config = read_project_config(workdir)?;
    config.skills.manual_added.retain(|skill| skill.path != slug);
    write_project_config(workdir, &config)?;
    sync_project_lock(workdir)
}

pub fn write_project_added_skills(workdir: &Path, lockfile: &Lockfile) -> Result<()> {
    let mut config = read_project_config(workdir)?;
    config.skills.manual_added = lockfile_to_project_added_skills(lockfile);
    write_project_config(workdir, &config)?;
    sync_project_lock(workdir)
}

pub fn write_project_preset(workdir: &Path, name: &str) -> Result<()> {
    write_project_presets(workdir, &[name.to_string()])
}

pub fn write_project_presets(workdir: &Path, names: &[String]) -> Result<()> {
    let mut bindings = read_project_bindings(workdir)?;
    bindings.presets.manual_added = normalize_unique_slugs(names.to_vec());
    write_project_bindings(workdir, &bindings)?;
    sync_project_lock(workdir)
}

pub fn enable_project_preset(workdir: &Path, name: &str) -> Result<()> {
    let slug = sanitize_slug(name);
    if slug.is_empty() {
        bail!("invalid preset name: {name}");
    }

    let mut presets = read_project_presets(workdir)?;
    if !presets.contains(&slug) {
        presets.push(slug);
    }
    write_project_presets(workdir, &presets)
}

pub fn disable_project_preset(workdir: &Path, name: &str) -> Result<()> {
    let mut presets = read_project_presets(workdir)?;
    presets.retain(|preset| preset != name);
    write_project_presets(workdir, &presets)
}

pub fn remove_project_preset(workdir: &Path) -> Result<()> {
    let mut config = read_project_config(workdir)?;
    config.presets.matched.clear();
    config.presets.manual_added.clear();
    config.presets.manual_skipped.clear();
    config.selectors.matched.clear();
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

    let installed_at = std::time::SystemTime::now()
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
            installed_at,
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
            installed_at,
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
    let had_manual_entry = config.skills.manual_added.iter().any(|skill| skill.path == slug);
    if had_manual_entry {
        remove_project_added_skill(workdir, &slug)?;
        removed_any = true;
    } else if removed_any {
        sync_project_lock(workdir)?;
    }

    Ok(removed_any)
}

// ---------------------------------------------------------------------------
// Preset CRUD helpers
// ---------------------------------------------------------------------------

pub fn create_preset(name: &str, description: Option<&str>) -> Result<()> {
    let sign = sanitize_slug(name);
    if sign.is_empty() {
        bail!("invalid preset name: {name}");
    }
    let mut store = read_presets_store()?;
    if store.presets.contains_key(&sign) {
        bail!("preset '{sign}' already exists");
    }
    store.presets.insert(
        sign.clone(),
        PresetConfig {
            sign: Some(sign.clone()),
            name: name.to_string(),
            description: description.map(String::from),
            skills: Vec::new(),
            flocks: Vec::new(),
        },
    );
    write_presets_store(&store)
}

pub fn delete_preset(name: &str) -> Result<()> {
    let mut store = read_presets_store()?;
    if store.presets.remove(name).is_none() {
        bail!("preset '{name}' not found");
    }
    write_presets_store(&store)
}

pub fn add_skills_to_preset(preset_name: &str, slugs: &[String]) -> Result<()> {
    let mut store = read_presets_store()?;
    let preset = store
        .presets
        .get_mut(preset_name)
        .with_context(|| format!("preset '{preset_name}' not found"))?;
    for slug in slugs {
        if !preset.skills.contains(slug) {
            preset.skills.push(slug.clone());
        }
    }
    write_presets_store(&store)
}

pub fn remove_skills_from_preset(preset_name: &str, slugs: &[String]) -> Result<()> {
    let mut store = read_presets_store()?;
    let preset = store
        .presets
        .get_mut(preset_name)
        .with_context(|| format!("preset '{preset_name}' not found"))?;
    preset.skills.retain(|s| !slugs.contains(s));
    write_presets_store(&store)
}

pub fn add_flocks_to_preset(preset_name: &str, flock_slugs: &[String]) -> Result<()> {
    let mut store = read_presets_store()?;
    let preset = store
        .presets
        .get_mut(preset_name)
        .with_context(|| format!("preset '{preset_name}' not found"))?;
    for slug in flock_slugs {
        if !preset.flocks.contains(slug) {
            preset.flocks.push(slug.clone());
        }
    }
    write_presets_store(&store)
}

pub fn remove_flocks_from_preset(preset_name: &str, flock_slugs: &[String]) -> Result<()> {
    let mut store = read_presets_store()?;
    let preset = store
        .presets
        .get_mut(preset_name)
        .with_context(|| format!("preset '{preset_name}' not found"))?;
    preset.flocks.retain(|f| !flock_slugs.contains(f));
    write_presets_store(&store)
}

// ---------------------------------------------------------------------------
// Skill resolution
// ---------------------------------------------------------------------------

/// Information about a resolved skill for MCP serving.
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
    pub presets: Vec<String>,
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

fn add_preset_skills(
    sources: &mut BTreeMap<String, ResolvedSkillSources>,
    store: &PresetsStore,
    preset_name: &str,
    selector_name: Option<&str>,
) {
    let Some(preset) = store.presets.get(preset_name) else {
        return;
    };

    for skill in &preset.skills {
        let entry = sources.entry(skill.clone()).or_default();
        add_unique(&mut entry.presets, preset_name);
        if let Some(selector_name) = selector_name {
            add_unique(&mut entry.selectors, selector_name);
        }
    }

    // Expand flocks referenced by this preset
    for flock_slug in &preset.flocks {
        if let Ok(skill_slugs) = crate::registry::list_flock_skill_slugs(flock_slug) {
            for skill_slug in skill_slugs {
                let entry = sources.entry(skill_slug).or_default();
                add_unique(&mut entry.presets, preset_name);
                add_unique(&mut entry.flocks, flock_slug);
                if let Some(selector_name) = selector_name {
                    add_unique(&mut entry.selectors, selector_name);
                }
            }
        }
    }
}

pub fn list_repo_skills() -> Result<Vec<RepoSkillFolder>> {
    find_repo_skill_folders(&repo_checkout_root())
}

fn project_skill_search_dirs(workdir: &Path) -> Vec<PathBuf> {
    let primary = project_skills_dir(workdir);
    let legacy = legacy_project_skills_dir(workdir);
    if primary == legacy {
        vec![primary]
    } else {
        vec![primary, legacy]
    }
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

    // 3. Repo-installed skills (from installed_skills.json)
    if let Ok(installed) = crate::registry::read_installed_skills_file() {
        for entry in installed {
            if let Some(repo_sign) = &entry.repo_sign {
                let path = PathBuf::from(repo_sign);
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
                repo: String::new(),
                path: skill.slug.clone(),
                version: version_info.version,
                commit_hash: version_info.git_commit,
            }
        })
        .collect::<Vec<_>>();

    ProjectLockFile { version: 1, skills }
}

fn resolve_project_skills_internal(
    workdir: &Path,
    override_preset: Option<&str>,
) -> Result<Vec<ResolvedProjectSkill>> {
    let bindings = read_project_bindings(workdir)?;
    let config = read_project_config(workdir)?;
    let store = read_presets_store()?;
    let mut sources = BTreeMap::<String, ResolvedSkillSources>::new();

    let explicit_presets = if let Some(name) = override_preset {
        normalize_unique_slugs([name.to_string()])
    } else {
        // Combine matched + manual_added, filter out manual_skipped
        let mut all_presets = bindings.presets.matched.clone();
        for slug in &bindings.presets.manual_added {
            if !all_presets.contains(slug) {
                all_presets.push(slug.clone());
            }
        }
        all_presets.retain(|s| !bindings.presets.manual_skipped.contains(s));
        all_presets
    };

    for preset_name in &explicit_presets {
        add_preset_skills(&mut sources, &store, preset_name, None);
    }

    if override_preset.is_none() {
        for matched in &bindings.selectors.matched {
            for preset_name in &matched.presets {
                add_preset_skills(
                    &mut sources,
                    &store,
                    preset_name,
                    Some(matched.selector.as_str()),
                );
            }
            // Expand flocks from each matched selector
            for flock_slug in &matched.flocks {
                if let Ok(skill_slugs) = crate::registry::list_flock_skill_slugs(flock_slug) {
                    for skill_slug in skill_slugs {
                        let entry = sources.entry(skill_slug).or_default();
                        add_unique(&mut entry.flocks, flock_slug);
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
    let resolved = resolve_project_skills_internal(workdir, None)?;
    write_project_lockfile(workdir, &build_project_lockfile(&resolved))
}

pub fn resolve_project_skills_with_sources(
    workdir: &Path,
    override_preset: Option<&str>,
) -> Result<Vec<ResolvedProjectSkill>> {
    let resolved = resolve_project_skills_internal(workdir, override_preset)?;
    if override_preset.is_none() {
        let _ = write_project_lockfile(workdir, &build_project_lockfile(&resolved));
    }
    Ok(resolved)
}

/// Resolve the list of skills for a project directory.
///
/// Priority:
/// 1. Explicit preset bindings
/// 2. Selector-matched presets
/// 3. Project-local manual skills from `savhub.toml`
///
/// Skill folders are looked up in the global skills directory.
pub fn resolve_skills_for_project(
    workdir: &Path,
    override_preset: Option<&str>,
) -> Result<Vec<ResolvedSkill>> {
    Ok(
        resolve_project_skills_with_sources(workdir, override_preset)?
            .into_iter()
            .map(|skill| ResolvedSkill {
                slug: skill.slug,
                display_name: skill.display_name,
                folder: skill.folder,
            })
            .collect(),
    )
}
