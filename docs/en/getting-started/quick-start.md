---
title: Quick Start
description: Install and set up the Savhub CLI and desktop app
---

# Quick Start

Savhub Client is a tool for discovering, fetching, and managing AI coding skills across your projects. It works with Claude Code, Codex, Cursor, Windsurf, and other AI agents.

## Installation

### CLI

Download the latest release from [GitHub Releases](https://github.com/savhub-ai/savhub/releases) or build from source:

```bash
# Clone and build
git clone https://github.com/savhub-ai/savhub.git
cd savhub
cargo build --release

# The binary is at target/release/savhub
```

### Desktop App

Download the desktop installer from the same release page, or build from source:

```bash
cargo build --release -p savhub-desktop
```

## Authentication

Login with your GitHub account to publish and manage skills:

```bash
savhub login
```

This opens a browser for GitHub OAuth. After authorization, your token is stored locally.

```bash
# Verify your login
savhub whoami

# Logout when needed
savhub logout
```

## Quick Start

### 1. Browse available skills

```bash
# Search for skills
savhub search rust

# Browse all skills
savhub explore

# List available flocks (skill collections)
savhub flock list
```

### 2. Apply skills to your project

Navigate to your project directory and run:

```bash
cd /path/to/my-project
savhub apply
```

This will:
1. Run selectors to detect your project type (e.g., Rust, Python, Web)
2. Show matched selectors and recommended flocks
3. Let you interactively select which flocks to fetch
4. Fetch skills and sync them to your AI clients (Claude Code, Codex, etc.)

### 3. Verify

```bash
# List fetched skills in the project
savhub list

# Check selector results without making changes
savhub selector test
```

## What's Next

- [Apply Command](https://savhub.ai/en/docs/client/apply) - Detailed usage of the apply workflow
- [Selectors](https://savhub.ai/en/docs/client/selectors) - Create custom project detection rules
