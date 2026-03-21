use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ResourceKind;

pub const MAX_BUNDLE_BYTES: usize = 10 * 1024 * 1024; // 10 MB
pub const BUNDLE_META_FILE: &str = "skill.toml";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleSourceKind {
    Prompt,
    Script,
    Tool,
    Bundle,
    Git,
}

impl Default for BundleSourceKind {
    fn default() -> Self {
        Self::Bundle
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BundleMetadata {
    #[serde(default)]
    pub format: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub source: BundleSourceInfo,
    #[serde(default)]
    pub runtime: BundleRuntime,
    #[serde(default)]
    pub discovery: BundleDiscovery,
    #[serde(default)]
    pub author: BundleAuthor,
    #[serde(default)]
    pub package: BundlePackage,
    #[serde(default)]
    pub links: IndexMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BundleGitInfo {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub reference: BundleGitReference,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BundleGitReference {
    #[serde(default)]
    pub kind: BundleSourceKind,
    #[serde(default)]
    pub value: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BundleSourceInfo {
    #[serde(default)]
    pub kind: BundleSourceKind,
    #[serde(default)]
    pub entry: Option<String>,
    #[serde(default)]
    pub git: Option<BundleGitInfo>,
    #[serde(default)]
    pub subdir: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InstallStep {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub package: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BundleRuntime {
    #[serde(default)]
    pub bins: Vec<String>,
    #[serde(default)]
    pub env: Vec<String>,
    #[serde(default)]
    pub network: Option<bool>,
    #[serde(default)]
    pub install: Vec<InstallStep>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BundleDiscovery {
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BundleAuthor {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BundlePackage {
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredBundleFile {
    pub path: String,
    pub content: String,
    pub size: i32,
    pub sha256: String,
}

pub fn is_slug(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-' || *b == b'_')
}

pub fn normalize_tags(tags: &[String]) -> Vec<String> {
    tags.iter()
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .collect()
}

pub fn normalize_bundle_files(
    files: &[crate::PublishBundleFile],
) -> Result<Vec<StoredBundleFile>, String> {
    let mut result = Vec::new();
    for file in files {
        let content = file.content.clone();
        let size = content.len() as i32;
        let sha256 = format!("{:x}", md5_ish(&content));
        result.push(StoredBundleFile {
            path: file.path.clone(),
            content,
            size,
            sha256,
        });
    }
    Ok(result)
}

fn md5_ish(data: &str) -> u64 {
    let mut hash: u64 = 0;
    for byte in data.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
    }
    hash
}

pub fn total_bundle_bytes(files: &[impl BundleFile]) -> usize {
    files.iter().map(|f| f.content().len()).sum()
}

impl From<StoredBundleFile> for crate::PublishBundleFile {
    fn from(stored: StoredBundleFile) -> Self {
        Self {
            path: stored.path,
            content: stored.content,
        }
    }
}

pub fn required_main_file(_kind: crate::ResourceKind) -> &'static str {
    BUNDLE_META_FILE
}

/// Trait for types that carry a file path and content string.
pub trait BundleFile {
    fn path(&self) -> &str;
    fn content(&self) -> &str;
}

impl BundleFile for StoredBundleFile {
    fn path(&self) -> &str {
        &self.path
    }
    fn content(&self) -> &str {
        &self.content
    }
}

impl BundleFile for crate::PublishBundleFile {
    fn path(&self) -> &str {
        &self.path
    }
    fn content(&self) -> &str {
        &self.content
    }
}

pub fn load_bundle_metadata(
    files: &[impl BundleFile],
    _kind: ResourceKind,
) -> Result<Option<BundleMetadata>, String> {
    for file in files {
        if file.path() == BUNDLE_META_FILE {
            return toml::from_str(file.content())
                .map(Some)
                .map_err(|e| format!("invalid {BUNDLE_META_FILE}: {e}"));
        }
    }
    Ok(None)
}

pub fn bundle_metadata_from_json(value: &Value) -> Result<BundleMetadata, String> {
    serde_json::from_value(value.clone()).map_err(|e| format!("invalid bundle metadata: {e}"))
}

/// Parse a TOML string into [`BundleMetadata`].
pub fn parse_bundle_metadata_toml(raw: &str) -> Result<BundleMetadata, String> {
    toml::from_str(raw).map_err(|e| format!("invalid TOML bundle metadata: {e}"))
}
