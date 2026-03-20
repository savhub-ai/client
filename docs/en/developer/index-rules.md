---
title: Index Rules
description: Control how repositories are scanned and grouped into flocks
---

# Index Rules

Index rules control how Savhub scans repositories and organizes skills into flocks. Rules are managed via the admin panel under **Management -> Index Rules**.

## Rule Structure

Each rule has:

| Field | Description |
|-------|-------------|
| **Repo URL** | Normalized git URL (e.g. `https://github.com/org/repo.git`) |
| **Path Regex** | Path inside the repo to scan (e.g. `skills`, `*`) |
| **Strategy** | Grouping algorithm: `each_dir_as_flock` or `smart` |

## Strategies

### `each_dir_as_flock`

Every matched directory becomes its own flock. No minimum size thresholds.

**Example**: For a repo structured as:
```
skills/
  python/
    SKILL.md
  rust/
    SKILL.md
  go/
    SKILL.md
```

With rule `path_regex: skills`, this produces 3 flocks: `python`, `rust`, `go`.

### `smart` (Default)

Uses an LCA (Lowest Common Ancestor) algorithm to auto-detect grouping structure. Groups are only created when there are at least 2 groups with at least 2 skills each. Otherwise falls back to a single flock.

## Path Regex Matching

When a user submits an index job for a repo:

1. The system looks up rules matching the normalized repo URL
2. If the user scanned from root (`subdir = "."`), a rule with a concrete `path_regex` (like `skills`) **overrides the scan root** to that path
3. If multiple rules exist for the same repo, the most specific match wins:
   - Exact match on subdir scores highest
   - Concrete paths score higher than wildcard `*`
4. If no rule matches, the default `smart` strategy is used with the original subdir

## Examples

| Repo URL | Path Regex | Strategy | Effect |
|----------|-------------|----------|--------|
| `https://github.com/anthropics/skills.git` | `skills` | `each_dir_as_flock` | Scan only `skills/` dir, each subdir = 1 flock |
| `https://github.com/openclaw/skills.git` | `*` | `each_dir_as_flock` | Scan from root, each top-level dir = 1 flock |
| *(no rule)* | - | `smart` | Auto-detect grouping via LCA algorithm |
