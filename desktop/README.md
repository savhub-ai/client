# Savhub Desktop

Desktop client for the Savhub AI Skills registry. Built with [Dioxus](https://dioxuslabs.com/) (Rust) using a WebView-based desktop backend.

## Overview

Savhub Desktop provides a visual interface for managing AI skills across projects and AI clients (Claude Code, Cursor, Windsurf, VS Code, Continue). It connects to the Savhub registry to browse, install, and update skills, and integrates with the MCP (Model Context Protocol) server to expose skills to AI clients.

## Architecture

```
savhub-client/
  cli/           # CLI tool (savhub command)
  desktop/       # Desktop GUI (this crate)
  mcp-server/    # MCP server binary (savhub-mcp)
  shared/        # Shared library (savhub-local) used by all crates
```

The desktop app is a single Rust binary (`savhub-desktop`) that renders with Dioxus Desktop (WebView2 on Windows, WebKit on macOS/Linux).

### Key crates

| Crate | Purpose |
|---|---|
| `savhub-desktop` | Desktop GUI application |
| `savhub-local` (shared/) | Shared logic: profiles, skills, config, selectors, registry cache |
| `savhub-mcp` (mcp-server/) | MCP server process, spawned by the desktop app |
| `savhub-shared` | Shared types from the server repository (API models) |

## Pages

The app uses a sidebar layout with the following pages:

### Dashboard (`/`)
- Shows registry connection status (with API version), logged-in user, and installed skill count.
- Registry API compatibility banner — if the registry API version is incompatible with this client, a non-dismissible warning appears at the top.
- Recent projects list with timestamps.
- Detected AI clients on the system.

### Skills (`/explore`)
- Search and browse skills from the Savhub registry.
- One-click install to the current project.

### Selectors (`/selectors`)

Selectors automatically identify project types by checking for files, folders, or glob patterns. When matched, they apply presets and skills to the project.

**Key features:**
- **Rule expression system**: Rules can be combined with AND (`&&`), OR (`||`), NOT (`!`), and parentheses for complex matching logic. Example: `(1 && 2) || !3`
- **Rule types**: File Exists, Folder Exists, Glob Match (e.g., `**/*.rs`)
- **Match modes**: All Match (AND all rules), Any Match (OR all rules), Custom expression
- **Create from scratch** or **use any existing selector as template**
- **Presets selection**: Toggle checkboxes to select from available presets
- **Skills selection**: Searchable checkbox list of known skills, plus manual text input for custom slugs
- **Modal editor**: Create/edit form opens as a centered overlay dialog

Four default selectors are seeded on first use: Rust Service, Web Frontend, Python Service, Monorepo Web App.

### Projects (`/projects`)
- **Left panel**: list of registered project directories. Add/remove project paths.
- **Right panel**: when a project is selected, shows:
  - Matched selectors and their contributed presets
  - Current presets (enabled/disabled toggles)
  - Enabled skills with source provenance (preset, selector, manual)
  - Repo skills dialog for adding skills from local repos

### Presets (`/presets`)
- Create, edit, and delete named skill presets.
- Each preset groups a set of skill slugs.
- Add/remove skills from presets.

### MCP Server (`/mcp`)
- Start/stop the `savhub-mcp` child process.
- Auto-register/unregister MCP server configuration in detected AI clients.
- Toggle auto-start on boot.
- View registration status per AI client.

### Settings (`/settings`)
- **General**: registry URL, working directory, language (English/Chinese).
- **Account**: GitHub OAuth login, token management.
- **About**: version info, update check, auto-update with download and restart.

## Registry Cache

The Savhub registry is a Git repo (`savhub-ai/registry`) containing JSON metadata for skills organized by repository and flock.

**Performance strategy:**
- On startup, the desktop app downloads the registry as a single zip archive (~1MB)
- All JSON metadata is parsed in memory and stored in a local SQLite database (`~/.config/savhub/registry.db`)
- Subsequent queries (search, list, detail) are served from SQLite — instant responses
- The commit SHA is tracked; if the registry hasn't changed, the download is skipped

**Data model** (matching the registry schema):
- **Repo**: a git repo containing skills
- **Flock**: a curated distribution of skills inside a repository
- **Skill**: individual skill record with metadata, source info, and entry point

## Registry API Version Compatibility

The client declares a `CLIENT_API_VERSION` constant. On startup, it queries the registry's `/health` endpoint for `apiVersion`. If the versions differ, a persistent red banner warns the user that data may be incompatible and they should update the client.

## Window Configuration

The default window size is set to **90% of the primary display resolution** (detected via `GetSystemMetrics` on Windows, fallback to 1440x900 on other platforms). All popup dialogs are designed to fit without scrollbars at this default size.

## Popup Dialogs

All popup dialogs use a consistent modal overlay pattern:
- Fixed backdrop with centered modal
- Close button (×) in the top-right corner
- Click backdrop to dismiss
- Content-appropriate sizing without scrollbars

Current dialogs:
- **Selector Editor**: create/edit selectors with rule builder and preset/skill selectors
- **Error Detail**: registry connection error details
- **Add Project Skill**: browse and enable repo skills with conflict resolution

## Skill Management

Skills are the core unit in Savhub. Each skill is a folder containing a `SKILL.md` manifest and related files (prompts, resources, tools for AI).

**Installation flow:**
1. User finds a skill on the Skills page and clicks Install.
2. The app downloads a ZIP bundle from the registry API.
3. The bundle is extracted to the global skills directory.
4. An entry is added to the project's config tracking the installed version.
5. An `origin.json` is written inside the skill folder with registry metadata.

## Presets

Presets are named groups of skill slugs stored globally in `~/.config/savhub/profiles.json`. A project can enable multiple presets via `savhub.toml`, which determines which skills are active.

**Skill resolution priority:**
1. Explicit preset bindings
2. Selector-matched presets
3. Project-local manual skills from `savhub.toml`

## Internationalization

The app supports English and Chinese. All UI strings are defined in `src/i18n.rs` using actual Chinese characters (not Unicode escapes) for readability. The language setting is stored in the global config.

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
| `profiles.json` | `~/.config/savhub/` | Preset definitions |
| `projects.json` | `~/.config/savhub/` | Known project directory list |
| `selectors.json` | `~/.config/savhub/` | Selector definitions with rule expressions |
| `registry.db` | `~/.config/savhub/` | Local registry SQLite cache |
| `savhub.toml` | `<project>/` | Project config: presets, matched selectors, manual skills |
| `savhub.lock` | `<project>/` | Locked skill versions |

## Theme

The app uses a "bamboo green" (竹叶青) color palette defined in `src/theme.rs`, matching the Savhub web frontend. All styling is inline CSS applied through Dioxus RSX.
