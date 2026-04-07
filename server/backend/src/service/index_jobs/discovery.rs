//! Pure SKILL discovery & flock-grouping helpers.
//!
//! Extracted from `index_jobs.rs` (refactor C1). These functions have **no DB
//! access and no I/O** apart from `extract_repo_description`'s README probe;
//! that lone exception is kept here because it conceptually belongs to the
//! discovery phase and is trivial to test in isolation.

use std::collections::HashMap;
use std::fs;

use super::super::git_ops::{SkillCandidate, sanitize_skill_slug};
use super::super::helpers::parse_git_url_parts;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FlockGroupPlan {
    pub(crate) slug: String,
    pub(crate) source_path: String,
    pub(crate) candidate_indices: Vec<usize>,
}

pub(crate) fn compute_each_dir_as_flock_plans(
    candidates: &[SkillCandidate],
    repo_name: &str,
) -> Vec<FlockGroupPlan> {
    // Each SKILL.md directory becomes its own flock.  The full relative
    // directory is used as the group key so that deeply-nested repos
    // (e.g. skills/<author>/<skill>/SKILL.md) produce one flock per leaf.
    let mut plans: Vec<FlockGroupPlan> = candidates
        .iter()
        .enumerate()
        .map(|(i, candidate)| {
            let dir = &candidate.relative_dir;
            let (slug, source_path) = if dir == "." {
                (sanitize_skill_slug(repo_name), ".".to_string())
            } else {
                (sanitize_skill_slug(dir), dir.clone())
            };
            FlockGroupPlan {
                slug,
                source_path,
                candidate_indices: vec![i],
            }
        })
        .collect();
    plans.sort_by(|a, b| a.slug.cmp(&b.slug));
    plans
}

// ---------------------------------------------------------------------------
// LCA-based skill grouping
// ---------------------------------------------------------------------------

pub(crate) fn path_segments(relative_dir: &str) -> Vec<&str> {
    if relative_dir == "." {
        vec![]
    } else {
        relative_dir.split('/').collect()
    }
}

pub(crate) fn join_repo_relative_path(base: &str, child: &str) -> String {
    match (base, child) {
        (".", ".") => ".".to_string(),
        (".", child) => child.to_string(),
        (base, ".") => base.to_string(),
        (base, child) => format!("{base}/{child}"),
    }
}

pub(crate) fn find_lca(candidates: &[SkillCandidate]) -> Vec<String> {
    let all_segs: Vec<Vec<&str>> = candidates
        .iter()
        .map(|c| path_segments(&c.relative_dir))
        .collect();

    if all_segs.is_empty() {
        return vec![];
    }

    let min_len = all_segs.iter().map(|s| s.len()).min().unwrap_or(0);
    let mut lca = Vec::new();

    for i in 0..min_len {
        let segment = all_segs[0][i];
        if all_segs.iter().all(|segs| segs[i] == segment) {
            lca.push(segment.to_string());
        } else {
            break;
        }
    }

    lca
}

fn group_source_path(
    lca: &[String],
    lca_len: usize,
    candidates: &[SkillCandidate],
    indices: &[usize],
) -> String {
    let Some(&index) = indices.first() else {
        return ".".to_string();
    };
    let segs = path_segments(&candidates[index].relative_dir);
    if segs.len() <= lca_len {
        return ".".to_string();
    }

    let mut parts = lca.to_vec();
    parts.push(segs[lca_len].to_string());
    if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    }
}

pub(crate) fn compute_flock_group_plans(
    candidates: &[SkillCandidate],
    repo_name: &str,
) -> Vec<FlockGroupPlan> {
    let lca = find_lca(candidates);
    let lca_len = lca.len();

    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();

    for (i, candidate) in candidates.iter().enumerate() {
        let segs = path_segments(&candidate.relative_dir);
        let key = if segs.len() > lca_len {
            sanitize_skill_slug(segs[lca_len])
        } else {
            sanitize_skill_slug(repo_name)
        };
        groups.entry(key).or_default().push(i);
    }

    let num_groups = groups.len();
    let max_size = groups
        .values()
        .map(|indices| indices.len())
        .max()
        .unwrap_or(0);

    if num_groups < 2 || max_size < 2 {
        return vec![FlockGroupPlan {
            slug: sanitize_skill_slug(repo_name),
            source_path: ".".to_string(),
            candidate_indices: (0..candidates.len()).collect(),
        }];
    }

    let mut plans = groups
        .into_iter()
        .map(|(slug, candidate_indices)| FlockGroupPlan {
            source_path: group_source_path(&lca, lca_len, candidates, &candidate_indices),
            slug,
            candidate_indices,
        })
        .collect::<Vec<_>>();
    plans.sort_by(|left, right| left.slug.cmp(&right.slug));
    plans
}

#[cfg(test)]
fn compute_flock_groups(
    candidates: &[SkillCandidate],
    repo_name: &str,
) -> HashMap<String, Vec<usize>> {
    compute_flock_group_plans(candidates, repo_name)
        .into_iter()
        .map(|plan| (plan.slug, plan.candidate_indices))
        .collect()
}

/// Dedup adjacent `/`-separated segments whose first word matches (case-insensitive),
/// replace `-` with space, join with space.
/// When the first word of the current segment equals the first word of the previous
/// segment, the previous segment is replaced by the current one.
/// e.g. `"anthropics/mcp-skills"` → `"anthropics mcp skills"`
/// e.g. `"mofa/mofa-skills"` → `"mofa skills"`
pub(crate) fn path_to_display_name(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let mut deduped: Vec<String> = Vec::new();
    for part in &parts {
        let expanded = part.replace('-', " ");
        let cur_first = expanded.split_whitespace().next().unwrap_or("");
        if let Some(prev_last) = deduped.last_mut() {
            let prev_first = prev_last.split_whitespace().next().unwrap_or("");
            if prev_first.eq_ignore_ascii_case(cur_first) {
                // Replace the previous segment with the current (more specific) one
                *prev_last = expanded;
                continue;
            }
        }
        deduped.push(expanded);
    }
    deduped.join(" ")
}

/// Build a human-readable repo name from the git URL path (domain stripped).
pub(crate) fn extract_repo_name(git_url: &str) -> String {
    let (_domain, path_slug) = parse_git_url_parts(git_url);
    if path_slug.is_empty() {
        return "imported".to_string();
    }
    let name = path_to_display_name(&path_slug);
    if name.is_empty() {
        "imported".to_string()
    } else {
        name
    }
}

/// Extract repo description from the README title (first `#` heading) in the checkout.
/// Falls back to `"Auto-created repo for {repo_name}"`.
pub(crate) fn extract_repo_description(checkout_path: &std::path::Path, repo_name: &str) -> String {
    let default = format!("Auto-created repo for {repo_name}");
    for candidate in &["README.md", "readme.md", "Readme.md", "README"] {
        let path = checkout_path.join(candidate);
        if let Ok(content) = fs::read_to_string(&path)
            && let Some(heading) = content.lines().find_map(|line| {
                let trimmed = line.trim();
                // Match `# Title` but not `##` or deeper
                if trimmed.starts_with("# ") {
                    let title = trimmed.trim_start_matches('#').trim();
                    if !title.is_empty() {
                        return Some(title.to_string());
                    }
                }
                None
            })
        {
            return heading;
        }
    }
    default
}

/// Derive a flock display name.  When the flock slug is just "skill" or "skills"
/// the flock inherits the repo name (optionally appended with the slug when the
/// repo name does not already end with skill/skills).
pub(crate) fn derive_flock_name(flock_slug: &str, repo_name: &str) -> String {
    let slug_lower = flock_slug.to_ascii_lowercase();
    if slug_lower == "skill" || slug_lower == "skills" {
        let repo_lower = repo_name.to_ascii_lowercase();
        if repo_lower.ends_with("skill") || repo_lower.ends_with("skills") {
            repo_name.to_string()
        } else {
            format!("{} {}", repo_name, flock_slug.replace('-', " "))
        }
    } else {
        flock_slug.replace('-', " ")
    }
}

/// Normalize a raw skill name from SKILL.md: expand hyphens to spaces and capitalize.
pub(crate) fn normalize_skill_name(original_name: &str) -> String {
    if !original_name.contains(' ') && original_name.contains('-') {
        capitalize_words(&original_name.replace('-', " "))
    } else {
        original_name.to_string()
    }
}

/// Derive a prefix from the flock name to prepend to skill names.
///
/// If the flock name ends with "skill" or "skills" (case-insensitive),
/// strip that suffix. The remaining words become the prefix.
/// e.g. "Rust Dev Skills" → "Rust Dev", "mofa skills" → "Mofa".
pub(crate) fn flock_name_prefix(flock_name: &str) -> String {
    let words: Vec<&str> = flock_name.split_whitespace().collect();
    let trimmed: Vec<&str> = if let Some(last) = words.last() {
        let lower = last.to_ascii_lowercase();
        if lower == "skill" || lower == "skills" {
            words[..words.len() - 1].to_vec()
        } else {
            words
        }
    } else {
        return String::new();
    };
    if trimmed.is_empty() {
        return String::new();
    }
    capitalize_words(&trimmed.join(" "))
}

/// Derive skill slug from the formatted skill name: lowercase, spaces → dashes.
pub(crate) fn format_skill_slug(formatted_name: &str) -> String {
    sanitize_skill_slug(formatted_name)
}

/// Words with special casing that should be preserved as-is.
const SPECIAL_CASE_WORDS: &[&str] = &[
    "iPhone",
    "iPad",
    "iPod",
    "iOS",
    "iMac",
    "iTunes",
    "iCloud",
    "macOS",
    "tvOS",
    "watchOS",
    "visionOS",
    "GitHub",
    "GitLab",
    "BitBucket",
    "DevOps",
    "DevTools",
    "JavaScript",
    "TypeScript",
    "GraphQL",
    "PostgreSQL",
    "MySQL",
    "SQLite",
    "MongoDB",
    "NoSQL",
    "OpenAI",
    "ChatGPT",
    "LangChain",
    "LlamaIndex",
    "FastAPI",
    "NextJS",
    "NodeJS",
    "NestJS",
    "VueJS",
    "ReactJS",
    "AngularJS",
    "OAuth",
    "WebSocket",
    "WebRTC",
    "gRPC",
    "VS",
    "VSCode",
    "YouTube",
    "LinkedIn",
    "TikTok",
    "WhatsApp",
    "WordPress",
    "MCP",
    "API",
    "APIs",
    "SDK",
    "CLI",
    "GUI",
    "URL",
    "URLs",
    "AI",
    "LLM",
    "LLMs",
    "NLP",
    "ML",
    "RAG",
    "AWS",
    "GCP",
    "CDN",
    "DNS",
    "SSH",
    "SSL",
    "TLS",
    "HTTP",
    "HTTPS",
    "JSON",
    "XML",
    "YAML",
    "TOML",
    "CSV",
    "HTML",
    "CSS",
    "CI",
    "CD",
    "QA",
    "ID",
    "UTF",
    "PDF",
    "SVG",
    "PNG",
    "JPG",
    "GIF",
    "eBay",
    "eBook",
];

fn capitalize_words(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            // Check if any special-case word matches (case-insensitive)
            if let Some(special) = SPECIAL_CASE_WORDS
                .iter()
                .find(|&&sc| sc.eq_ignore_ascii_case(w))
            {
                return special.to_string();
            }
            // Default: capitalize first letter
            let mut chars = w.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    format!("{}{}", upper, chars.as_str())
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn candidate(relative_dir: &str) -> SkillCandidate {
        SkillCandidate {
            path: PathBuf::from(relative_dir),
            relative_dir: relative_dir.to_string(),
        }
    }

    #[test]
    fn test_path_segments() {
        assert_eq!(path_segments("."), Vec::<&str>::new());
        assert_eq!(
            path_segments("skills/lang/python"),
            vec!["skills", "lang", "python"]
        );
        assert_eq!(path_segments("tools"), vec!["tools"]);
    }

    #[test]
    fn test_join_repo_relative_path() {
        assert_eq!(join_repo_relative_path(".", "."), ".");
        assert_eq!(
            join_repo_relative_path(".", "skills/python"),
            "skills/python"
        );
        assert_eq!(join_repo_relative_path("skills", "."), "skills");
        assert_eq!(join_repo_relative_path("skills", "python"), "skills/python");
    }

    #[test]
    fn test_find_lca_no_common() {
        let candidates = vec![candidate("coding/assistant"), candidate("devops/deployer")];
        assert_eq!(find_lca(&candidates), Vec::<String>::new());
    }

    #[test]
    fn test_find_lca_shared_prefix() {
        let candidates = vec![
            candidate("skills/lang/python"),
            candidate("skills/lang/rust"),
            candidate("skills/devops/deploy"),
        ];
        assert_eq!(find_lca(&candidates), vec!["skills"]);
    }

    #[test]
    fn test_find_lca_deep_shared_prefix() {
        let candidates = vec![
            candidate("src/skills/frontend/react"),
            candidate("src/skills/frontend/vue"),
            candidate("src/skills/backend/api"),
        ];
        assert_eq!(find_lca(&candidates), vec!["src", "skills"]);
    }

    #[test]
    fn test_find_lca_with_root_forces_empty() {
        let candidates = vec![
            candidate("."),
            candidate("skills/lang/python"),
            candidate("skills/lang/rust"),
        ];
        assert_eq!(find_lca(&candidates), Vec::<String>::new());
    }

    #[test]
    fn test_single_skill_at_root() {
        let candidates = vec![candidate(".")];
        let groups = compute_flock_groups(&candidates, "my-tool");
        assert_eq!(groups.len(), 1);
        assert!(groups.contains_key("my-tool"));
        assert_eq!(groups["my-tool"], vec![0]);
    }

    #[test]
    fn test_deep_nesting_with_categories() {
        let candidates = vec![
            candidate("skills/lang/python"),
            candidate("skills/lang/rust"),
            candidate("skills/devops/deploy"),
        ];
        let groups = compute_flock_groups(&candidates, "toolbox");
        assert_eq!(groups.len(), 2);
        assert_eq!(groups["lang"].len(), 2);
        assert_eq!(groups["devops"].len(), 1);
    }

    #[test]
    fn test_no_common_prefix_multi_flock() {
        let candidates = vec![
            candidate("coding/assistant"),
            candidate("coding/reviewer"),
            candidate("devops/deployer"),
        ];
        let groups = compute_flock_groups(&candidates, "repo");
        assert_eq!(groups.len(), 2);
        assert_eq!(groups["coding"].len(), 2);
        assert_eq!(groups["devops"].len(), 1);
    }

    #[test]
    fn test_quality_check_fallback_all_singletons() {
        let candidates = vec![
            candidate("skills/python"),
            candidate("skills/rust"),
            candidate("skills/go"),
        ];
        let groups = compute_flock_groups(&candidates, "tools");
        assert_eq!(groups.len(), 1);
        assert!(groups.contains_key("tools"));
        assert_eq!(groups["tools"].len(), 3);
    }

    #[test]
    fn test_mixed_root_and_nested() {
        let candidates = vec![
            candidate("."),
            candidate("tools/debug"),
            candidate("tools/lint"),
        ];
        let groups = compute_flock_groups(&candidates, "my-repo");
        assert_eq!(groups.len(), 2);
        assert_eq!(groups["my-repo"].len(), 1);
        assert_eq!(groups["tools"].len(), 2);
    }

    #[test]
    fn test_flat_siblings_single_each() {
        let candidates = vec![
            candidate("coding-assistant"),
            candidate("code-reviewer"),
            candidate("test-writer"),
        ];
        let groups = compute_flock_groups(&candidates, "skills");
        assert_eq!(groups.len(), 1);
        assert!(groups.contains_key("skills"));
        assert_eq!(groups["skills"].len(), 3);
    }

    #[test]
    fn test_deep_common_prefix_with_categories() {
        let candidates = vec![
            candidate("src/skills/frontend/react"),
            candidate("src/skills/frontend/vue"),
            candidate("src/skills/backend/api"),
        ];
        let groups = compute_flock_groups(&candidates, "repo");
        assert_eq!(groups.len(), 2);
        assert_eq!(groups["frontend"].len(), 2);
        assert_eq!(groups["backend"].len(), 1);
    }

    #[test]
    fn test_group_plans_capture_source_paths() {
        let candidates = vec![
            candidate("skills/lang/python"),
            candidate("skills/lang/rust"),
            candidate("skills/devops/deploy"),
        ];

        let plans = compute_flock_group_plans(&candidates, "toolbox");

        assert_eq!(plans.len(), 2);
        assert_eq!(plans[0].slug, "devops");
        assert_eq!(plans[0].source_path, "skills/devops");
        assert_eq!(plans[1].slug, "lang");
        assert_eq!(plans[1].source_path, "skills/lang");
    }

    #[test]
    fn test_group_plans_fallback_to_repo_root_path() {
        let candidates = vec![
            candidate("skills/python"),
            candidate("skills/rust"),
            candidate("skills/go"),
        ];

        let plans = compute_flock_group_plans(&candidates, "toolbox");

        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].slug, "toolbox");
        assert_eq!(plans[0].source_path, ".");
    }

    #[test]
    fn path_to_display_name_dedupes_repeated_prefix() {
        assert_eq!(path_to_display_name("mofa/mofa-skills"), "mofa skills");
    }

    #[test]
    fn path_to_display_name_keeps_distinct_segments() {
        assert_eq!(
            path_to_display_name("anthropics/mcp-skills"),
            "anthropics mcp skills"
        );
    }

    #[test]
    fn path_to_display_name_handles_single_segment() {
        assert_eq!(path_to_display_name("skills"), "skills");
    }

    #[test]
    fn path_to_display_name_ignores_empty_segments() {
        assert_eq!(path_to_display_name("/foo//bar/"), "foo bar");
    }

    #[test]
    fn path_to_display_name_case_insensitive_dedup() {
        assert_eq!(path_to_display_name("Foo/foo-bar"), "foo bar");
    }
}
