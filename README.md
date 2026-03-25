# Savhub

English | [中文](README.zh.md)

**Savhub is a project-aware AI skill manager.**

## Why Savhub?

AI coding agents (Claude Code, Codex, Cursor, etc.) are increasingly powerful, but they lack deep understanding of specific frameworks and toolchains. The community has written countless AI skills (prompts, rules, workflows) for this — but they're scattered everywhere, hard to discover and reuse.

Savhub solves this: **automatically match the right AI skills to the right project.**

## How It Works

```
savhub apply
```

1. **Selectors analyze your project** — check files and dependencies (Cargo.toml → Rust, package.json + react → React, ...)
2. **Match skill collections** — recommend matching flocks from the registry
3. **You choose what to install** — interactive selection
4. **Sync to your agents** — skills are written to Claude Code, Codex, Cursor, and other agent skill directories

Selectors are the core mechanism. Savhub ships with built-in selectors for common languages and frameworks (Rust, Python, Go, Salvo, Dioxus, React, Vue, etc.). You can also create custom selectors to match any project structure.

## Supported AI Agents

| Agent | Skills Directory |
|-------|-----------------|
| Claude Code | `.claude/skills/` |
| Codex | `.agents/skills/` |
| Cursor | Supported |
| Windsurf | Supported |
| Continue | Supported |
| VS Code (Copilot) | Supported |

## Quick Start

### Install

**Linux / macOS:**
```sh
curl -fsSL https://raw.githubusercontent.com/savhub-ai/savhub/main/scripts/install.sh | bash
```

**Windows (PowerShell):**
```powershell
irm https://raw.githubusercontent.com/savhub-ai/savhub/main/scripts/install.ps1 | iex
```

Or download binaries from [GitHub Releases](https://github.com/savhub-ai/savhub/releases).

### Usage

```bash
cd your-project
savhub apply          # Detect project → recommend skills → sync to agents
```

```bash
savhub login          # GitHub OAuth (needed to publish and star skills)
savhub search <query> # Search skills
savhub explore        # Browse the registry
savhub self-update    # Update the CLI
```

## Documentation

Full docs and CLI reference at [savhub.ai/docs](https://savhub.ai/en/docs).

## License

[Apache-2.0](LICENSE)
