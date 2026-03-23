use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::SecuritySummary;

fn is_default_security(summary: &SecuritySummary) -> bool {
    *summary == SecuritySummary::default()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryFlock {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub sign: String,
    #[serde(default, alias = "repo_sign")]
    pub repo: String,
    #[serde(default)]
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(default)]
    pub license: String,
    #[serde(default, skip_serializing_if = "is_default_security")]
    pub security: SecuritySummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistrySkill {
    pub slug: String,
    #[serde(default)]
    pub path: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default, skip_serializing_if = "is_default_security")]
    pub security: SecuritySummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataSource {
    Local,
    Remote,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillEntry {
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub status: String,
    pub license: String,
    pub categories: Vec<String>,
    pub keywords: Vec<String>,
    pub stars: Option<u32>,
    pub starred_by_me: Option<bool>,
    pub downloads: Option<u64>,
    pub owner: Option<String>,
    #[serde(default, skip_serializing_if = "is_default_security")]
    pub security: SecuritySummary,
    #[serde(skip)]
    pub data_source: Option<DataSource>,
}

impl From<RegistrySkill> for SkillEntry {
    fn from(skill: RegistrySkill) -> Self {
        Self {
            slug: skill.slug,
            name: skill.name,
            description: skill.description,
            version: skill.version,
            status: skill.status,
            license: skill.license,
            categories: skill.categories,
            keywords: skill.keywords,
            stars: None,
            starred_by_me: None,
            downloads: None,
            owner: None,
            security: skill.security,
            data_source: Some(DataSource::Remote),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FetchedSkillEntry {
    pub slug: String,
    pub fetched_at: String,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_path: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockEntry {
    pub version: String,
    pub fetched_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_slug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flock_slug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_sha: Option<String>,
}

/// Lock file format: `{ "version": 1, "<repo_url>": { "<skill_path>": LockEntry } }`
///
/// Uses `#[serde(flatten)]` so repo URLs appear as top-level keys alongside `version`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lockfile {
    pub version: u8,
    #[serde(flatten)]
    pub repos: BTreeMap<String, BTreeMap<String, LockEntry>>,
}

impl Default for Lockfile {
    fn default() -> Self {
        Self {
            version: 1,
            repos: BTreeMap::new(),
        }
    }
}

impl Lockfile {
    /// Insert an entry keyed by repo_url + path.
    pub fn insert(&mut self, repo_url: &str, path: &str, entry: LockEntry) {
        self.repos
            .entry(repo_url.to_string())
            .or_default()
            .insert(path.to_string(), entry);
    }

    /// Check if any entries exist.
    pub fn is_empty(&self) -> bool {
        self.repos.values().all(|paths| paths.is_empty())
    }

    /// Iterate over all entries as `(repo_url, path, &LockEntry)`.
    pub fn iter_entries(&self) -> impl Iterator<Item = (&str, &str, &LockEntry)> {
        self.repos.iter().flat_map(|(repo_url, paths)| {
            paths
                .iter()
                .map(move |(path, entry)| (repo_url.as_str(), path.as_str(), entry))
        })
    }

    /// Find an entry by slug (remote_slug or path basename).
    pub fn find_by_slug(&self, slug: &str) -> Option<(&str, &str, &LockEntry)> {
        for (repo_url, paths) in &self.repos {
            for (path, entry) in paths {
                let entry_slug = entry
                    .remote_slug
                    .as_deref()
                    .unwrap_or_else(|| path.rsplit('/').next().unwrap_or(path));
                if entry_slug == slug {
                    return Some((repo_url.as_str(), path.as_str(), entry));
                }
            }
        }
        None
    }

    /// Remove an entry by slug. Returns true if found.
    pub fn remove_by_slug(&mut self, slug: &str) -> bool {
        let mut found = false;
        for paths in self.repos.values_mut() {
            let before = paths.len();
            paths.retain(|path, entry| {
                let entry_slug = entry
                    .remote_slug
                    .as_deref()
                    .unwrap_or_else(|| path.rsplit('/').next().unwrap_or(path));
                entry_slug != slug
            });
            if paths.len() < before {
                found = true;
            }
        }
        self.repos.retain(|_, paths| !paths.is_empty());
        found
    }

    /// Count total entries.
    pub fn len(&self) -> usize {
        self.repos.values().map(|p| p.len()).sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoSkillOrigin {
    pub version: u8,
    pub repo: String,
    pub repo_sign: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_commit: Option<String>,
    pub slug: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_version: Option<String>,
    pub fetched_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteSkillFetchSpec {
    pub repo_sign: String,
    pub skill_path: String,
    pub git_url: String,
    pub git_sha: String,
    pub skill_version: Option<String>,
}
