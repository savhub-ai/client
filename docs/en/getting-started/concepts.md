---
title: Core Concepts
description: What Savhub is, key terminology, and how the pieces fit together
---

# Core Concepts

## What Is Savhub?

Savhub is a package manager for AI coding skills. It scans public Git repositories for `SKILL.md` files, organizes them into installable collections called *flocks*, and lets you fetch them into any project with a single command. It works with Claude Code, Codex, Cursor, Windsurf, and other AI-powered editors.

The platform has two parts:

- **savhub.ai** — the web registry where skills are browsed, searched, starred, and commented on.
- **Savhub Client** — a CLI (and desktop app) that detects your project type and syncs the right skills to your AI editors.

## Key Terms

### Skill

A skill is a Markdown file (`SKILL.md`) that teaches an AI agent how to do something — coding conventions, framework patterns, deployment checklists, etc. Each skill lives inside a Git repository and is versioned together with the source code it relates to.

### Flock

A flock is a collection of related skills that belong together. When a repository is indexed, Savhub automatically groups its skills into one or more flocks based on directory structure and content. You typically fetch flocks rather than individual skills.

### Repo

A repo is a registered Git repository. Savhub clones it, scans for `SKILL.md` files, and creates flocks from the results. A repo can contain many flocks.

### Selector

A selector is a rule that detects what kind of project you are working in — e.g., "Cargo.toml exists" means it's a Rust project. The Savhub Client runs selectors during `savhub apply` to recommend the right flocks for your project.

### Registry

The registry stores metadata about every indexed skill and flock. The Savhub Client queries the registry API so searches are fast.

## How It Works

```
Git Repo ──index──▶ Savhub Server ──publish──▶ Registry
                                                  │
                       savhub.ai ◀── browse ──────┘
                                                  │
                    Your Project ◀── savhub apply ─┘
```

1. A repository owner (or the Savhub crawler) submits a Git URL for indexing.
2. The server clones the repo, discovers `SKILL.md` files, groups them into flocks, and stores everything in the database.
3. Metadata is pushed to the public registry repo.
4. Users browse skills on savhub.ai, or search locally via `savhub search`.
5. Running `savhub apply` in a project detects the project type, recommends matching flocks, and fetches skills into the working directory.

## What's Next

- [Quick Start](https://savhub.ai/en/docs/getting-started/quick-start) — install the client and apply your first skills
- [Apply Command](https://savhub.ai/en/docs/client/apply) — detailed apply workflow
- [CLI Reference](https://savhub.ai/en/docs/client/cli-reference) — full command list
