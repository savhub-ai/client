---
title: Configuration
description: Configuration files and settings for the Savhub client
---

# Configuration

## Global Configuration Files

All global configuration is stored in `~/.config/savhub/` (or the platform-specific config directory).

| File | Description |
|------|-------------|
| `config.json` | Auth token, registry URL, language preference |
| `selectors.json` | Selector definitions for project type detection |
| `projects.json` | Registered project directories |
| `fetched_skills.json` | Tracking data for fetched skills |

### User Config Override

An optional `~/.savhub/config.toml` can override the REST API base URL:

```toml
[rest_api]
base_url = "https://custom-registry.example.com"
```

## Project Configuration

Savhub uses two files at the project root: `savhub.toml` (user intent) and `savhub.lock` (actual installed state).

### savhub.toml — Project Configuration

The config file has three top-level sections: `[selectors]`, `[flocks]`, and `[skills]`. Each section supports:

- **`matched`** — Auto-managed by `savhub apply`, replaced on each run.
- **`manual_added`** — User-added entries, never modified by `savhub apply`.
- **`manual_skipped`** — User-excluded entries, never modified by `savhub apply`.

The effective result for each section is: `matched + manual_added - manual_skipped`.

```toml
version = 1

# ── Selectors ──────────────────────────────────────────────
[selectors]

# Auto-managed by `savhub apply`:
[[selectors.matched]]
selector = "Rust Project"
flocks = ["rust-dev"]

[[selectors.matched]]
selector = "Salvo Web Framework"
flocks = ["salvo-skills"]

# User overrides (optional):
# manual_added = ["my-custom-selector"]
# manual_skipped = ["unwanted-selector"]

# ── Flocks ─────────────────────────────────────────────────
[flocks]

# Auto-managed: flocks contributed by matched selectors.
matched = ["rust-dev", "salvo-skills"]

# User overrides (optional):
# manual_added = ["my-private-flock"]
# manual_skipped = ["salvo-skills"]

# ── Skills ─────────────────────────────────────────────────
[skills]
# layout = "flat"    # or "flock"

# User-manually-added skills (via `savhub fetch`, etc.)
# Never modified by `savhub apply`.
# [[skills.manual_added]]
# sign = "github.com/anthropics/skills/skills/claude-api"
# slug = "claude-api"
# version = "1.0.0"

# Skills to never auto-fetch. Supports slugs and signs.
# manual_skipped = [
#     "some-unwanted-skill",
#     "github.com/owner/repo/skills/another-skill",
# ]
```

#### Section Details

**`[selectors]`** — Each `[[selectors.matched]]` entry records which selector matched and what flocks it contributed. Use `manual_skipped` to suppress a selector.

**`[flocks]`** — `matched` lists flocks contributed by selectors. Use `manual_added` to always fetch a flock. Use `manual_skipped` to exclude a flock even if a selector contributes it.

**`[skills]`** — `manual_added` lists skills the user explicitly fetched. `manual_skipped` lists skills that should never be auto-fetched by `savhub apply`.

#### Skill Signs

A skill "sign" uniquely identifies a skill by its registry path:

```
github.com/owner/repo/path/to/skill-slug
```

Signs can be used in `manual_added` (as the `sign` field) and in `manual_skipped` entries. Both plain slugs and full signs are supported.

#### Skills Layout

- **flat** (default) — Skills at `skills/{slug}/`
- **flock** — Skills grouped at `skills/{flock-slug}/{skill-slug}/`

### savhub.lock — Fetched State

The lock file records exactly which skills were fetched by `savhub apply`, including their versions and git commits. This is the source of truth for what is currently fetched.

When `savhub apply` runs:
- **Selectors match** — New skills are appended to the lock file.
- **No selectors match** — The lock file is read to determine which skills to remove, then deleted.

```toml
version = 1

[[skills]]
path = "rust-clippy"
version = "1.2.0"
git_sha = "abc123def456"

[[skills]]
path = "rust-testing"
version = "0.8.1"
git_sha = "789def012345"
```

> **Tip:** You can check `savhub.lock` into version control so teammates get the same skill versions.

## AI Client Integration

Skills are copied directly to AI client project-level directories by `savhub apply`:

| AI Client | Skills Directory |
|-----------|-----------------|
| Claude Code | `.claude/skills/` |
| Codex | `.agents/skills/` |
| Cursor | Not supported (uses `.mdc` rule format) |
| Windsurf | Not supported (uses different format) |

## Repository Cache

Skill source repositories are cloned to `~/.savhub/repos/`. The `savhub apply` command uses sparse checkout to minimize disk usage, cloning only the needed skill directories. Multiple skills from the same repository share a single clone.

## Backward Compatibility

The following legacy formats are automatically migrated:

| Legacy | Current |
|--------|---------|
| `detectors.json` | `selectors.json` |
| `savhub.toml` with `[detectors]` | `[selectors]` (serde alias) |
| `.savhub/lock.json` | `savhub.toml` skills section |
