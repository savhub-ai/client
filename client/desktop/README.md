# Savhub Desktop

Desktop client for the Savhub AI Skills registry. Built with [Dioxus](https://dioxuslabs.com/) (Rust) using a WebView-based desktop backend.

## Overview

Savhub Desktop provides a visual interface for managing AI skills across projects and AI clients (Claude Code, Codex, Cursor, Windsurf, VS Code, Continue). It connects to the Savhub registry to browse, fetch, and update skills.

## Architecture

```
client/
  cli/           # CLI tool (savhub command)
  desktop/       # Desktop GUI (this crate)
  local/         # Shared library (savhub-local) used by CLI and Desktop
```

The desktop app is a single Rust binary (`savhub-desktop`) that renders with Dioxus Desktop (WebView2 on Windows, WebKit on macOS/Linux).

## Pages

### Dashboard (`/`)
- Registry connection status (with API version) and logged-in user.
- API compatibility banner — warns if the registry API version is incompatible with this client.
- Recent projects list with timestamps.
- Detected AI clients on the system.

### Skills (`/explore`)
- Search and browse skills from the Savhub registry.
- One-click fetch to the current project.

### Selectors (`/selectors`)
Selectors automatically identify project types by checking for files, folders, or glob patterns. When matched, they recommend flocks and skills for the project.

- **Rule expression system**: Combine rules with AND (`&&`), OR (`||`), NOT (`!`), and parentheses
- **Rule types**: File Exists, Folder Exists, Glob Match
- **Match modes**: All Match, Any Match, Custom expression
- **Create from scratch** or **use any existing selector as template**
- **Modal editor**: Create/edit form as a centered overlay dialog

### Projects (`/projects`)
- **Left panel**: registered project directories (add/remove)
- **Right panel**: matched selectors, active flocks, installed skills with source provenance

### Settings (`/settings`)
- **General**: registry URL, working directory, language (English/Chinese)
- **Account**: GitHub OAuth login, token management
- **About**: version info, update check, auto-update

## Registry Data

The desktop app reads skills and flocks directly from the configured Savhub REST API.

**Data model** (matching the registry schema):
- **Repo**: a git repo containing skills
- **Flock**: a curated group of skills inside a repository
- **Skill**: individual skill record with metadata and source info

## Building

```bash
# Prerequisites: Rust 1.94+, WebView2 runtime (Windows)

# Development build
cargo run --package savhub-desktop

# Release build
cargo build --release --package savhub-desktop
```

The release binary is at `target/release/savhub-desktop` (or `.exe` on Windows).

## Configuration Files

| File | Location | Purpose |
|---|---|---|
| `config.json` | `~/.config/savhub/` | Global settings (registry URL, token, language) |
| `projects.json` | `~/.config/savhub/` | Known project directory list |
| `selectors.json` | `~/.config/savhub/` | Selector definitions with rule expressions |
| `savhub.toml` | `<project>/` | Project config: matched selectors, flocks, skills |
| `savhub.lock` | `<project>/` | Locked skill versions |

## Internationalization

The app supports English and Chinese. All UI strings are defined in `src/i18n.rs`. The language setting is stored in the global config.

## Theme

The app uses a "bamboo green" color palette defined in `src/theme.rs`, matching the Savhub web frontend.
