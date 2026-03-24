use serde::{Deserialize, Serialize};

use crate::SecuritySummary;

fn is_default_security(summary: &SecuritySummary) -> bool {
    *summary == SecuritySummary::default()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryFlock {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default, alias = "repo_url")]
    pub repo: String,
    #[serde(default)]
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub path: String,
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
pub struct LockSkill {
    pub path: String,
    pub slug: String,
    pub version: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockFlock {
    pub path: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<LockSkill>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockRepo {
    pub git_url: String,
    #[serde(default)]
    pub git_sha: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flocks: Vec<LockFlock>,
}

/// Lock file format:
/// ```json
/// {
///   "version": 1,
///   "repos": [
///     {
///       "git_url": "github.com/owner/repo",
///       "git_sha": "abc123",
///       "flocks": [{ "path": "skills/foo", "skills": [{ "path": "skills/foo", "slug": "foo", "version": "1.0" }] }]
///     }
///   ]
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lockfile {
    pub version: u8,
    #[serde(default)]
    pub repos: Vec<LockRepo>,
}

impl Default for Lockfile {
    fn default() -> Self {
        Self {
            version: 1,
            repos: Vec::new(),
        }
    }
}

impl Lockfile {
    /// Insert a skill into the lockfile under the given repo and flock.
    pub fn insert(&mut self, git_url: &str, git_sha: &str, flock_path: &str, skill: LockSkill) {
        let repo = match self.repos.iter_mut().find(|r| r.git_url == git_url) {
            Some(r) => {
                if !git_sha.is_empty() {
                    r.git_sha = git_sha.to_string();
                }
                r
            }
            None => {
                self.repos.push(LockRepo {
                    git_url: git_url.to_string(),
                    git_sha: git_sha.to_string(),
                    flocks: Vec::new(),
                });
                self.repos.last_mut().unwrap()
            }
        };
        let flock = match repo.flocks.iter_mut().find(|f| f.path == flock_path) {
            Some(f) => f,
            None => {
                repo.flocks.push(LockFlock {
                    path: flock_path.to_string(),
                    skills: Vec::new(),
                });
                repo.flocks.last_mut().unwrap()
            }
        };
        if let Some(existing) = flock.skills.iter_mut().find(|s| s.slug == skill.slug) {
            *existing = skill;
        } else {
            flock.skills.push(skill);
        }
    }

    /// Check if any entries exist.
    pub fn is_empty(&self) -> bool {
        self.repos
            .iter()
            .all(|r| r.flocks.iter().all(|f| f.skills.is_empty()))
    }

    /// Iterate over all skills as `(git_url, flock_path, &LockSkill)`.
    pub fn iter_skills(&self) -> impl Iterator<Item = (&str, &str, &str, &LockSkill)> {
        self.repos.iter().flat_map(|repo| {
            repo.flocks.iter().flat_map(move |flock| {
                flock.skills.iter().map(move |skill| {
                    (
                        repo.git_url.as_str(),
                        repo.git_sha.as_str(),
                        flock.path.as_str(),
                        skill,
                    )
                })
            })
        })
    }

    /// Find a skill by slug.
    pub fn find_by_slug(&self, slug: &str) -> Option<(&str, &str, &LockSkill)> {
        for repo in &self.repos {
            for flock in &repo.flocks {
                for skill in &flock.skills {
                    if skill.slug == slug {
                        return Some((repo.git_url.as_str(), flock.path.as_str(), skill));
                    }
                }
            }
        }
        None
    }

    /// Remove a skill by slug. Returns true if found.
    pub fn remove_by_slug(&mut self, slug: &str) -> bool {
        let mut found = false;
        for repo in &mut self.repos {
            for flock in &mut repo.flocks {
                let before = flock.skills.len();
                flock.skills.retain(|s| s.slug != slug);
                if flock.skills.len() < before {
                    found = true;
                }
            }
            repo.flocks.retain(|f| !f.skills.is_empty());
        }
        self.repos.retain(|r| !r.flocks.is_empty());
        found
    }

    /// Count total skill entries.
    pub fn len(&self) -> usize {
        self.repos
            .iter()
            .flat_map(|r| &r.flocks)
            .map(|f| f.skills.len())
            .sum()
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
