---
title: Scan & Grouping
description: How skills are discovered and organized into flocks
---

# Scan & Grouping

When a user submits a Git repo URL, the backend clones the repo, discovers all `SKILL.md` files, and automatically groups them into **flocks** under a **repo**.

## Pipeline Overview

```
POST /api/v1/index { git_url, git_ref, git_subdir }
  |
  +- 1. Clone repo               (10%)
  +- 2. Resolve index rules       (20%)
  +- 3. Locate SKILL.md files     (30%)
  +- 4. Group into flocks          (50%)  <- this document
  +- 5. Generate AI metadata       (if multi-skill flock)
  +- 6. Persist repo/flocks/skills (70%)
  +- 7. Run security scans
  +- 8. Sync registry checkout     (95%)
  +- 9. Finalize                   (100%)
```

Progress is broadcast to the frontend via WebSocket (pub/sub per `job_id`).

## Terminology

| Term | Meaning |
|------|---------|
| **Repo** | Top-level namespace, one per git repository |
| **Flock** | A group of skills within a repo, maps to a directory subtree |
| **Skill** | A single `SKILL.md` file with its enclosing directory |
| **`relative_dir`** | Path from the scan root to the skill directory. `"."` = root |
| **LCA** | Longest Common Ancestor -- the shared path prefix across all skill paths |

## Strategy Selection

Before grouping, the system checks the `index_rules` table for a matching rule:

1. Match the normalized repo URL against `repo_url`
2. If matched, the rule's `path_regex` may override the scan root
3. The rule's `strategy` determines the grouping algorithm

| Strategy | Behavior |
|----------|----------|
| `each_dir_as_flock` | Every matched directory becomes its own flock |
| `smart` (default) | LCA-based algorithm described below |

## Smart Grouping Algorithm (LCA-based)

### Step 1 -- Parse path segments

Each `relative_dir` is split into segments:

```
"."                    -> []
"skills/lang/python"   -> ["skills", "lang", "python"]
"skills/lang/rust"     -> ["skills", "lang", "rust"]
"devops/deploy"        -> ["devops", "deploy"]
```

### Step 2 -- Find the LCA

Compute the longest common prefix shared by all candidate paths:

```
["skills", "lang", "python"]
["skills", "lang", "rust"]
["skills", "devops", "deploy"]
-> LCA = ["skills"]

["skills", "lang", "python"]
["skills", "lang", "rust"]
[]                              <- root skill
-> LCA = []   (root forces empty prefix)
```

### Step 3 -- Assign group keys

Strip the LCA and take the first remaining segment as the group key:

- `segments.len() > lca_len` -> `sanitize(segments[lca_len])`
- Otherwise -> `sanitize(repo_name)`

### Step 4 -- Quality check

| Condition | Result |
|-----------|--------|
| `num_groups >= 2` AND `max_group_size >= 2` | **Multi flock** -- use computed groups |
| Otherwise | **Single flock** -- all skills under one flock named after the repo |

If every group has exactly 1 skill, the grouping isn't representing real categories.

## Examples

### Single skill at root

```
repo: github.com/alice/my-tool
+-- SKILL.md
```
Result: repo `my-tool`, flock `my-tool`, 1 skill

### Flat collection (quality check triggers)

```
repo: github.com/bob/skills
+-- coding-assistant/SKILL.md
+-- code-reviewer/SKILL.md
+-- test-writer/SKILL.md
```
3 groups but max_size=1 -> **single flock**: `skills` with 3 skills

### Deep nesting with categories

```
repo: github.com/dave/toolbox
+-- skills/lang/python/SKILL.md
+-- skills/lang/rust/SKILL.md
+-- skills/devops/deploy/SKILL.md
```
LCA=`["skills"]`, groups: `lang`(2), `devops`(1) -> **multi flock**

### each_dir_as_flock strategy

```
repo: github.com/anthropics/skills (rule: path_regex="skills")
+-- skills/python/SKILL.md
+-- skills/rust/SKILL.md
+-- skills/go/SKILL.md
```
Scan root overridden to `skills/`, each subdir = 1 flock: `python`, `rust`, `go`

## WebSocket Updates

```json
// Subscribe
{"action": "subscribe", "job_id": "019..."}

// Progress event
{"type": "index_progress", "job_id": "019...", "status": "running",
 "progress_pct": 50, "progress_message": "Categorizing skills..."}

// Unsubscribe
{"action": "unsubscribe", "job_id": "019..."}
```

## Implementation Reference

- LCA grouping: `backend/src/service/index_jobs.rs` -> `compute_flock_group_plans()`
- each_dir_as_flock: `backend/src/service/index_jobs.rs` -> `compute_each_dir_as_flock_plans()`
- Index rules: `backend/src/service/index_rules.rs` -> `resolve_index_rule()`
- Skill discovery: `backend/src/service/git_ops.rs` -> `collect_skill_candidates()`
- AI metadata: `backend/src/service/ai.rs` -> `generate_flock_metadata()`
