---
title: CLI Reference
description: Complete Savhub CLI command reference
---

# CLI Reference

## Global Options

All commands accept these global options:

| Option | Description |
|--------|-------------|
| `--workdir <path>` | Project directory (default: current directory) |
| `--dir <path>` | Skills directory within workdir |
| `--site <url>` | API site URL |
| `--registry <url>` | Registry URL |
| `--no-input` | Disable interactive prompts |

## Authentication

```bash
savhub login [--no-browser]       # Login via GitHub OAuth
savhub logout                      # Clear local auth token
savhub whoami                      # Show current user
savhub auth login|logout|whoami    # Auth subcommands
```

## Apply (Auto-Configuration)

```bash
savhub apply [options]
```

| Option | Description |
|--------|-------------|
| `--dry-run` | Preview changes without applying |
| `--yes` | Skip all prompts |
| `--agents <list>` | Only sync to these AI agents |
| `--skip-agents <list>` | Skip these AI agents |
| `--skills <list>` | Manually add skills |
| `--skip-skills <list>` | Manually skip skills |
| `--flocks <list>` | Manually add flocks |
| `--skip-flocks <list>` | Manually skip flocks |

Alias: `savhub auto`

## Skills

```bash
savhub search <query...> [--limit N]            # Search registry
savhub fetch <slug> [--version V] [--force]      # Fetch a skill
savhub update [slug] [--all] [--global] [--force] # Update skill(s)
savhub prune <slug> [--yes]                      # Prune a skill
savhub list                                       # List fetched skills
savhub explore [--limit N] [--sort S] [--json]   # Browse skills
savhub inspect <slug> [options]                  # View skill details
```

### Inspect Options

| Option | Description |
|--------|-------------|
| `--version <V>` | Show specific version |
| `--tag <TAG>` | Filter by tag |
| `--versions` | Show version history |
| `--files` | List files |
| `--file <PATH>` | Show file content |
| `--json` | JSON output |

## Enable / Disable

```bash
savhub enable <slug> --repo <path> [options]   # Enable repo skill in project
savhub disable <slug> [--yes]                  # Disable project skill
```

### Enable Options

| Option | Description |
|--------|-------------|
| `--repo <path>` | Repository name |
| `--selector <S>` | Associate with selector(s) |
| `--use-repo` | Overwrite existing skill |
| `--keep-existing` | Keep existing skill on conflict |

## Selectors

```bash
savhub selector list              # List all selectors
savhub selector show <name>       # Show selector details
savhub selector test              # Run selectors against current dir
```

Alias: `savhub detector`

## Flocks

```bash
savhub flock list                     # List all flocks
savhub flock show <slug>              # Show flock details
savhub flock fetch <slug> [--yes]     # Fetch flock skills
```

## Registry

```bash
savhub registry search <query...> [--limit N]           # Search registry
savhub registry list [--page N] [--page-size N] [--json] # List skills
```

## Self-Update

```bash
savhub self-update       # Update the CLI to the latest version
```

Checks GitHub Releases for a newer version, downloads the platform-specific binary, and replaces the current executable in place. A backup of the old binary is created (`.old`) and automatically cleaned up on the next run.

## Other

```bash
savhub docs              # Open documentation in browser
```
