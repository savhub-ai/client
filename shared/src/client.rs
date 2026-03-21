use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::SecuritySummary;

fn is_default_security(summary: &SecuritySummary) -> bool {
    *summary == SecuritySummary::default()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitRef {
    pub r#type: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RegistrySource {
    Git {
        url: String,
        r#ref: GitRef,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        commit_hash: Option<String>,
    },
    Registry {
        path: String,
    },
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
    pub source: Option<RegistrySource>,
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
            source: None,
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
    pub sign: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lockfile {
    pub version: u8,
    pub skills: BTreeMap<String, LockEntry>,
}

impl Default for Lockfile {
    fn default() -> Self {
        Self {
            version: 1,
            skills: BTreeMap::new(),
        }
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
    pub git_rev: String,
    pub skill_version: Option<String>,
}
