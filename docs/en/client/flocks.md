---
title: Flocks
description: Browse and fetch skill collections from the registry
---

# Flocks

Flocks are curated collections of related skills published in the Savhub registry. A flock groups skills that work well together (e.g., all Rust development skills in one "rust-dev" flock).

## CLI Commands

```bash
# List all available flocks
savhub flock list

# Show flock details and contained skills
savhub flock show rust-dev

# Fetch all skills from a flock
savhub flock fetch rust-dev

# Fetch without confirmation
savhub flock fetch rust-dev --yes
```

## How Flocks Work

### Registry-Defined

Flocks are defined in the Savhub registry by repository maintainers. Each flock has:
- **Slug** - Unique identifier (e.g., `rust-dev`)
- **Name** - Display name (e.g., "Rust Development")
- **Description** - What the flock covers
- **Skills** - List of skill slugs in this flock

### Browsing Flocks

```bash
$ savhub flock list
  rust-dev                   5 skill(s)  Rust Development
    Tools and best practices for Rust projects
  web-frontend               8 skill(s)  Web Frontend
    Modern web frontend development skills
  python-ml                  6 skill(s)  Python ML
    Machine learning with Python

3 flock(s)
```

### Fetching Flocks

When you fetch a flock, each skill is cloned from its git source and tracked in `savhub.toml`:

```bash
$ savhub flock fetch rust-dev
Flock: Rust Development (rust-dev)
Skills to fetch:
  [+] rust-clippy
  [+] rust-testing
  [+] rust-error-handling
Fetch 3 skill(s) from flock "rust-dev"? [Y/n]
  Added: rust-clippy
  Added: rust-testing
  Added: rust-error-handling

Done. 3 skill(s) added from flock "rust-dev".
```

## Flocks in the Ecosystem

Flocks integrate with other Savhub features:

- **Selectors** can reference flocks - when a selector matches, its flocks are suggested
- **Apply command** - `savhub apply` collects flocks from selectors, then lets you choose which to fetch

## Desktop App

The Savhub Desktop app provides flock browsing on the **Skills** page (grouped view) and detailed flock pages showing all contained skills with fetch/prune actions.
