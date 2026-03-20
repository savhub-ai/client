---
name: savhub-cli
description: Use the Savhub CLI to discover, fetch, manage, and apply AI skills for the current project.
---

# savhub-cli

You are an AI assistant that can use the **savhub** CLI to discover, fetch, manage, and apply AI coding skills for the current project.

## Quick Reference

```
savhub                              # Auto-detect project & apply skills (alias for savhub apply)
savhub apply [options]              # Detect project type, fetch matching skills, sync to AI clients
savhub search <query>               # Search skills in the registry
savhub fetch <slug> [--version V]   # Fetch a skill from the registry
savhub prune <slug> [--yes]         # Remove a fetched skill
savhub update [slug] [--all]        # Update fetched skill(s) to latest version
savhub list                         # List fetched skills in the current project
savhub explore [--limit N] [--sort] # Browse all skills from the registry API
savhub inspect <slug> [--json]      # View detailed info about a skill
savhub flock list                   # List available skill collections (flocks)
savhub flock show <slug>            # Show flock details and contained skills
savhub flock fetch <slug> [--yes]   # Fetch all skills from a flock
savhub selector list                # List all configured selectors
savhub selector test                # Test selectors against current directory
savhub selector show <name>         # Show selector details
savhub login                        # Login via GitHub OAuth
savhub logout                       # Clear local auth token
savhub whoami                       # Show current authenticated user
```

## Core Workflows

### 1. Apply skills to a project (recommended)

The fastest way to set up skills for a project. Runs selectors to detect the project type, shows matching flocks, and lets the user choose what to fetch.

```bash
cd /path/to/project
savhub apply
```

With options:
```bash
savhub apply --dry-run              # Preview without changes
savhub apply --yes                  # Skip prompts (CI/automation)
savhub apply --agents claude-code   # Only sync to Claude Code
savhub apply --flocks rust-dev      # Manually add a flock
savhub apply --skip-skills legacy   # Exclude a specific skill
```

Agent names for `--agents` / `--skip-agents`: `claude-code`, `codex`, `cursor`, `windsurf`, `continue`, `vscode`.

### 2. Fetch individual skills

```bash
savhub search rust                  # Find skills
savhub fetch rust-clippy            # Fetch one
savhub fetch rust-clippy --version 1.2.0  # Specific version
savhub list                         # Verify
```

### 3. Fetch a flock (skill collection)

```bash
savhub flock list                   # Browse flocks
savhub flock show rust-dev          # See what's inside
savhub flock fetch rust-dev         # Fetch all skills in the flock
```

### 4. Update and prune

```bash
savhub update rust-clippy           # Update one skill
savhub update --all                 # Update all fetched skills
savhub prune rust-clippy            # Remove a skill
```

## Project Files

| File | Location | Purpose |
|------|----------|---------|
| `savhub.toml` | `<project>/` | Project config: matched selectors, manual skills, flock bindings |
| `savhub.lock` | `<project>/` | Locked skill versions (auto-managed by `savhub apply`) |
| `config.toml` | `~/.savhub/` | Global settings: registry URL, auth token, data directory |
| `selectors.json` | `~/.savhub/` | Selector rules for project type detection |

### savhub.toml structure

```toml
[selectors]
# matched = [...]  # Auto-populated by savhub apply

[flocks]
# matched = [...]          # Auto-populated by savhub apply
# manual_added = ["rust-dev"]      # Always fetch this flock
# manual_skipped = ["legacy-flock"] # Never fetch this flock

[skills]
# layout = "flat"          # or "flock" for grouped layout
# manual_added = [...]     # Skills added via savhub fetch
# manual_skipped = ["unwanted-skill"]  # Never auto-fetch
```

All `manual_*` fields are persistent and never overwritten by `savhub apply`.

## When to Use Each Command

| User intent | Command |
|-------------|---------|
| "Set up skills for this project" | `savhub apply` |
| "Find skills for React" | `savhub search react` |
| "Add the clippy skill" | `savhub fetch clippy` |
| "Remove a skill I don't need" | `savhub prune <slug>` |
| "Update everything" | `savhub update --all` |
| "What skills do I have?" | `savhub list` |
| "What flocks are available?" | `savhub flock list` |
| "Check what selectors match here" | `savhub selector test` |
| "Preview what apply would do" | `savhub apply --dry-run` |
