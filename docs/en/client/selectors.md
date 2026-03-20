---
title: Selectors
description: Configure project type detection rules
---

# Selectors

Selectors are rules that automatically detect your project type by checking files, folders, and environment conditions. When matched, they apply presets, add skills, or fetch flocks.

## Overview

A selector consists of:
- **Name** - Display name (e.g., "Rust Project")
- **Rules** - One or more conditions to check
- **Match mode** - How rules are combined (all, any, or custom expression)
- **Actions** - Presets to enable, skills to add, and flocks to fetch
- **Priority** - Higher priority selectors take precedence

## CLI Commands

```bash
# List all configured selectors
savhub selector list

# Show details of a selector (partial name match)
savhub selector show rust

# Test selectors against current directory (no changes)
savhub selector test
```

The `savhub selector` command is also available as `savhub detector` for backward compatibility.

## Rule Types

| Rule | Description | Example |
|------|-------------|---------|
| **File Exists** | Check that a file exists | `Cargo.toml` |
| **Folder Exists** | Check that a folder exists | `src/` |
| **Glob Match** | Match files with a glob pattern | `**/*.rs` |
| **File Contains** | Check that a file contains a substring | `path: package.json`, `contains: react` |
| **File Regex** | Match file content with a regex | `path: Cargo.toml`, `pattern: salvo` |
| **Env Var Set** | Check an environment variable is set | `RUST_LOG` |
| **Command Exits** | Check a shell command exits with code 0 | `which cargo` |

All file/folder paths are relative to the selector's **folder scope** (default: `.`, the project root).

## Match Modes

### All Match
All rules must be true (AND logic). This is the default.

### Any Match
At least one rule must be true (OR logic).

### Custom Expression
Write a boolean expression referencing rules by number (1-indexed):

```
(1 && 2) || !3
```

Supported operators:
- `&&` - AND
- `||` - OR
- `!` - NOT
- `()` - Grouping

Example: A selector with 3 rules using `(1 && 2) || 3` means "rules 1 AND 2 must match, OR rule 3 alone is sufficient."

## Actions

When a selector matches, it can:

1. **Enable presets** - Activate named skill groups
2. **Add skills** - Directly add individual skills by slug
3. **Add flocks** - Fetch entire skill collections

## Priority

Selectors have an integer priority (default: 0). Higher values run first. When multiple selectors match:
- All matched selectors contribute their presets, skills, and flocks
- The union of all actions is applied

## Default Selectors

On first use, Savhub creates a set of default selectors for common project types. You can edit or delete them in the desktop app or by editing `selectors.json` directly.

## Managing Selectors

Selectors are best managed through the **Savhub Desktop** app, which provides a visual editor with:
- Rule builder UI
- Expression editor
- Preset/skill/flock search
- Template support

Selectors are stored in `~/.config/savhub/selectors.json`. The file is automatically migrated from the legacy `detectors.json` if present.

## Example Selector

A Rust project selector:

```json
{
  "id": "rust-project",
  "name": "Rust Project",
  "description": "Detects Rust/Cargo projects",
  "folder_scope": ".",
  "rules": [
    { "kind": "file_exists", "path": "Cargo.toml" }
  ],
  "match_mode": "all_match",
  "custom_expression": "",
  "presets": ["rust-core"],
  "add_skills": [],
  "add_flocks": ["rust-dev"],
  "priority": 10
}
```
