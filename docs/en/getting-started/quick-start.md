---
title: Quick Start
description: Install the Savhub CLI and apply your first AI skills
---

# Quick Start

Savhub is a project-aware AI skill manager. It analyzes your project's characteristics through **selectors** — built-in rules that detect your languages, frameworks, and project structure — and automatically installs the matching AI skills to your coding agents (Claude Code, Codex, Cursor, Windsurf, etc.).

## Install

### One-Line Install (Recommended)

**Linux / macOS:**
```bash
curl -fsSL https://raw.githubusercontent.com/savhub-ai/savhub/main/scripts/install.sh | bash
```

**Windows (PowerShell):**
```powershell
irm https://raw.githubusercontent.com/savhub-ai/savhub/main/scripts/install.ps1 | iex
```

The installer downloads the latest release, adds `savhub` to your PATH, and installs bundled skills into your AI agents.

### Manual Download

Download binaries from [GitHub Releases](https://github.com/savhub-ai/savhub/releases) and place them in your PATH.

### Build from Source

```bash
git clone https://github.com/savhub-ai/savhub.git
cd savhub
cargo build --release
# Binary: target/release/savhub (or savhub.exe on Windows)
```

## Apply Skills to Your Project

Navigate to your project and run:

```bash
cd /path/to/my-project
savhub apply
```

This will:
1. Run **selectors** to analyze your project (e.g., `Cargo.toml` → Rust, `package.json` with `react` → React)
2. Show matched selectors and recommend matching **flocks** (skill collections)
3. Let you interactively select which flocks to install
4. Fetch skills and copy them to your AI agents (Claude Code, Codex, etc.)

Savhub ships with built-in selectors for common languages and frameworks. You can also create **custom selectors** to match any project structure — see [Selectors](https://savhub.ai/en/docs/client/selectors).

## Browse and Search Skills

```bash
# Search for skills by keyword
savhub search rust

# Browse all skills from the registry
savhub explore

# List available flocks (skill collections)
savhub flock list

# View a specific flock and its skills
savhub flock show rust-dev
```

## Login (Optional)

Login with GitHub to star skills and publish your own:

```bash
savhub login       # Opens browser for GitHub OAuth
savhub whoami      # Verify your login
savhub logout      # Clear local token
```

## Verify

```bash
# List skills installed in the current project
savhub list

# Test selectors without making changes
savhub selector test

# Preview what apply would do
savhub apply --dry-run
```

## What's Next

- [Core Concepts](https://savhub.ai/en/docs/getting-started/concepts) - Key terminology and how the pieces fit together
- [Apply Command](https://savhub.ai/en/docs/client/apply) - Detailed usage of the apply workflow
- [Selectors](https://savhub.ai/en/docs/client/selectors) - Create custom project detection rules
- [CLI Reference](https://savhub.ai/en/docs/client/cli-reference) - Full command list
