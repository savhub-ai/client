---
title: Apply Command
description: Auto-detect project type and apply skills to AI clients
---

# Apply Command

The `savhub apply` command is the primary way to configure skills for a project. It runs selectors to detect your project type, resolves matching flocks and skills, and fetches them directly to your AI clients' skill directories.

## Basic Usage

```bash
cd /path/to/my-project
savhub apply
```

## How It Works

### When Selectors Match

1. **Selector matching** — All configured selectors run against the current directory. Each selector checks rules (file existence, glob patterns, etc.) to detect the project type.

2. **Flock collection** — Matched selectors contribute flocks (skill collections).

3. **Interactive selection** — You choose which flocks to fetch via a multi-select dialog (skipped with `--yes`).

4. **Skipped filtering** — Skills listed in `savhub.toml` `[skills] skipped` are excluded. Entries can be slugs or signs (see [Skill Signs](#skill-signs)).

5. **Diff against lockfile** — Skills already recorded in `savhub.lock` are skipped; only new skills are fetched.

6. **Batch fetch** — All new skills are fetched in a single batch operation, grouping by git repository to minimize clone/pull operations.

7. **Direct copy** — Skills are copied from the repo checkout directly to each AI client's project-level skills directory:
   - Claude Code: `.claude/skills/`
   - Codex: `.agents/skills/`

8. **File updates:**
   - `savhub.toml` — The `matched` fields in `[selectors]` and `[flocks]` are **replaced** with the current selector results. All `manual_*` fields are **never modified** by apply.
   - `savhub.lock` — Fetched skills are **appended** with their version and git commit.

### When No Selectors Match

If no selectors match, all skills previously applied by savhub will be removed:

1. Reads `savhub.lock` to determine which skills were fetched.
2. Lists skills to remove and asks for confirmation (unless `--yes`).
3. Removes skill folders from AI client directories (`.claude/skills/`, `.agents/skills/`).
4. Clears `selectors.matched` and `flocks.matched` in `savhub.toml` (all `manual_*` fields are untouched).
5. Deletes `savhub.lock`.

## Options

| Option | Description |
|--------|-------------|
| `--dry-run` | Show what would be done without making changes |
| `--yes`, `-y` | Skip all confirmation prompts (accept all flocks) |
| `--agents <list>` | Only sync to specific AI agents |
| `--skip-agents <list>` | Skip specific AI agents |
| `--skills <list>` | Manually add skills by slug or sign (saved to `skills.manual_added`) |
| `--skip-skills <list>` | Manually skip skills (saved to `skills.manual_skipped`) |
| `--flocks <list>` | Manually add flocks (saved to `flocks.manual_added`) |
| `--skip-flocks <list>` | Manually skip flocks (saved to `flocks.manual_skipped`) |

All `--skills`, `--flocks` and their `--skip-*` counterparts are **persistent** — they are saved to `savhub.toml` and apply on every subsequent run.

## Skill Signs

A skill "sign" is the full registry path that uniquely identifies a skill:

```
github.com/owner/repo/path/to/skill
```

Signs can be used in `savhub.toml` `[skills] manual_skipped` to exclude specific skills from auto-fetch. Both plain slugs and signs are supported:

```toml
[skills]
manual_skipped = [
    "some-skill",                                          # by slug
    "github.com/anthropics/skills/skills/claude-api",      # by full sign
]
```

## Examples

```bash
# Preview changes without applying
savhub apply --dry-run

# Apply without prompts (CI/automation)
savhub apply -y

# Only fetch for Claude Code
savhub apply --agents claude-code

# Fetch for all agents except Cursor
savhub apply --skip-agents cursor

# Multiple agents (comma-separated)
savhub apply --agents claude-code,codex

# Manually add a flock
savhub apply --flocks rust-dev

# Manually add a skill by slug
savhub apply --skills my-skill

# Skip a specific skill from auto-fetch
savhub apply --skip-skills unwanted-skill

# Combine: add a flock and skip one of its skills
savhub apply --flocks web-dev --skip-skills legacy-tool
```

## Agent Names

Use these names with `--agents` and `--skip-agents`:

| Name | AI Client |
|------|-----------|
| `claude-code` | Claude Code |
| `codex` | Codex (OpenAI) |
| `cursor` | Cursor |
| `windsurf` | Windsurf |
| `continue` | Continue |
| `vscode` | VS Code |

## Output Example

```
Matched selectors (by priority):
  [+] Rust Project (priority 10) — Detects Rust/Cargo projects

Flocks to fetch:
  [+] rust-dev (5 skills)

Skills to fetch:
  [+] rust-clippy
  [+] rust-testing

  ✓ rust-clippy -> .claude/skills/rust-clippy
  ✓ rust-testing -> .claude/skills/rust-testing
  ✓ rust-clippy -> .agents/skills/rust-clippy
  ✓ rust-testing -> .agents/skills/rust-testing

Done. 2 skill(s) fetched from 1 flock(s), 1 selector(s) matched.
```

## Backward Compatibility

The command is also available as `savhub auto` (alias).
