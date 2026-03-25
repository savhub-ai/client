# Savhub

**Savhub is a package manager for AI coding skills.** It discovers `SKILL.md` files from public Git repositories, organizes them into installable collections called *flocks*, and syncs them to your AI coding agents with a single command.

Think of it as npm/pip for AI skills: you run `savhub apply` in your project, and it automatically detects your tech stack (Rust, Python, React, etc.) and installs the right skills for your AI editor.

> **Status:** Under active development. Features may be incomplete or subject to change.

## How It Works

```
1. Git repos with SKILL.md files get indexed on savhub.ai
2. Skills are grouped into flocks (e.g. "rust-dev", "web-frontend")
3. You run `savhub apply` in your project
4. Savhub detects your project type via selectors
5. Matching skills are fetched and synced to your AI agents
```

## Supported AI Agents

Skills are synced to whichever agents are installed on your machine:

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

Or download binaries directly from [GitHub Releases](https://github.com/savhub-ai/savhub/releases).

### Apply Skills to a Project

```bash
cd your-project
savhub apply
```

This detects your project type via selectors, shows matching flocks, and lets you choose which to install. Skills are then synced to your AI agents.

### Login (Optional)

```bash
savhub login     # GitHub OAuth — needed to publish and star skills
savhub whoami    # Verify login
```

## CLI Commands

### Core Workflow

| Command | Description |
|---|---|
| `savhub` | Detect project & apply skills (default) |
| `savhub apply` | Same as above, with flags (`--dry-run`, `--yes`, `--agents`, etc.) |
| `savhub search <query>` | Search skills in the registry |
| `savhub fetch <slug>` | Fetch a skill by cloning its source repo |
| `savhub update` | Update project skills from local cache |
| `savhub prune <slug>` | Remove a skill |
| `savhub list` | List fetched skills in the current project |

### Discovery

| Command | Description |
|---|---|
| `savhub explore` | Browse skills from the registry |
| `savhub inspect <slug>` | View detailed skill info |
| `savhub flock list` | List available flocks |
| `savhub flock show <slug>` | Show flock details and skills |
| `savhub flock fetch <slug>` | Fetch all skills from a flock |

### Selectors

Selectors detect project types (e.g. "Cargo.toml exists" = Rust project). Built-in selectors cover Rust, Python, Go, Java, and frameworks like Salvo, Dioxus, Makepad, React, Vue, Angular, Next.js, and more.

```bash
savhub selector list        # List all selectors
savhub selector test        # Test selectors against current directory
savhub selector show <name> # Show selector details
```

### Skill Cache Management

| Command | Description |
|---|---|
| `savhub fetched` | List globally fetched skills |
| `savhub fetched --update` | Update all fetched repos/skills to latest |
| `savhub fetched --prune` | Remove skills not used by any project |

### Bundled Skills (Pilot)

| Command | Description |
|---|---|
| `savhub pilot install` | Install bundled skills into AI agent directories |
| `savhub pilot uninstall` | Remove bundled skills |
| `savhub pilot status` | Show installation status per agent |

### Other

| Command | Description |
|---|---|
| `savhub login` | Login via GitHub OAuth |
| `savhub logout` | Clear local auth token |
| `savhub whoami` | Show current authenticated user |
| `savhub self-update` | Update the CLI to the latest version |
| `savhub docs` | Open documentation in browser |

### Global Options

| Option | Description |
|---|---|
| `--profile <path>` | Config/data directory (overrides default `~/.savhub`) |
| `--workdir <path>` | Project directory (default: current directory) |
| `--dir <path>` | Skills directory within workdir |
| `--site <url>` | API site URL |
| `--registry <url>` | Registry URL |
| `--no-input` | Disable interactive prompts |

## Configuration

### Client

Global config is stored at `~/.config/savhub/`:

| File | Description |
|------|-------------|
| `config.json` | Auth token, registry URL, language preference |
| `selectors.json` | Selector definitions for project type detection |
| `projects.json` | Registered project directories |
| `fetched_skills.json` | Tracking data for fetched skills |

An optional `~/.savhub/config.toml` can override the REST API base URL:

```toml
[rest_api]
base_url = "https://custom-registry.example.com"
```

Environment variables: `SAVHUB_REGISTRY`, `SAVHUB_CONFIG_DIR`.

### Project Files

Each project uses two files at the project root:

- **`savhub.toml`** - Project configuration: matched selectors, flocks, and manual overrides
- **`savhub.lock`** - Exact versions of fetched skills (can be committed to version control)

### Server

All server configuration is via environment variables. Copy `.env.example` to `.env` and fill in values.

#### Required

| Variable | Description | Example |
|----------|-------------|---------|
| `DATABASE_URL` | PostgreSQL connection string | `postgres://postgres:postgres@127.0.0.1:45432/savhub_dev` |
| `SAVHUB_GITHUB_CLIENT_ID` | GitHub OAuth app client ID | `Ov23li...` |
| `SAVHUB_GITHUB_CLIENT_SECRET` | GitHub OAuth app secret | `70733a...` |
| `SAVHUB_GITHUB_REDIRECT_URL` | GitHub OAuth callback URL | `http://127.0.0.1:5006/api/v1/auth/github/callback` |

#### Server Settings

| Variable | Description | Default |
|----------|-------------|---------|
| `SAVHUB_BIND` | Backend listen address | `127.0.0.1:5006` |
| `SAVHUB_FRONTEND_ORIGIN` | Frontend URL for CORS | `http://127.0.0.1:5007` |
| `SAVHUB_API_BASE` | Public API base URL | `http://{SAVHUB_BIND}/api/v1` |
| `SAVHUB_SPACE_PATH` | Data directory for repo caches | `./space` |

#### Background Worker

| Variable | Description | Default |
|----------|-------------|---------|
| `SAVHUB_SYNC_INTERVAL_SECS` | Flock sync interval | `300` |
| `SAVHUB_SYNC_STALE_HOURS` | Hours before a flock is considered stale | `6` |
| `SAVHUB_AUTO_INDEX_MIN_INTERVAL_SECS` | Min interval between auto-index per repo | `3600` |

#### AI Metadata Generation

| Variable | Description | Default |
|----------|-------------|---------|
| `SAVHUB_AI_PROVIDER` | AI provider: `zhipu` or `doubao` | *(disabled)* |
| `SAVHUB_AI_API_KEY` | API key for the provider | |
| `SAVHUB_AI_CHAT_MODEL` | Override default model | `glm-4-flash` (zhipu) / `doubao-1-5-pro-32k-250115` (doubao) |

#### User Roles

| Variable | Description |
|----------|-------------|
| `SAVHUB_GITHUB_ADMIN_LOGINS` | Comma-separated GitHub logins granted admin role on first login |
| `SAVHUB_GITHUB_MODERATOR_LOGINS` | Comma-separated GitHub logins granted moderator role on first login |

## Components

| Crate | Path | Binary | Description |
|---|---|---|---|
| `savhub-backend` | `server/backend` | `savhub-backend` | Backend API server (Salvo + Diesel + PostgreSQL) |
| `savhub-frontend` | `server/frontend` | *(WASM)* | Web frontend (Dioxus) |
| `savhub-shared` | `shared` | *(library)* | Shared types between server and clients |
| `savhub-local` | `client/local` | *(library)* | Client-side logic: selectors, registry cache, client detection |
| `savhub` | `client/cli` | `savhub` | Command-line interface |
| `savhub-desktop` | `client/desktop` | `savhub-desktop` | Desktop GUI (Dioxus native) |

## Stack

| Layer | Technology |
|-------|-----------|
| Frontend | Dioxus 0.7 (WASM) |
| Backend | Salvo 0.89 |
| ORM | Diesel 2.3 |
| Database | PostgreSQL 18 |
| Desktop | Dioxus native |
| CLI TUI | Ratatui |

## Development

Requires **Rust 1.94+**.

### Server + Web Frontend

```bash
# 1. Start PostgreSQL
docker compose up -d postgres

# 2. Copy and configure environment
cp .env.example .env
# Edit .env: fill in SAVHUB_GITHUB_CLIENT_ID, SAVHUB_GITHUB_CLIENT_SECRET

# 3. Run backend
cargo run -p savhub-backend

# 4. Run frontend (separate terminal)
cd server/frontend && dx serve --platform web --port 5007
```

Open http://127.0.0.1:5007

### Client (CLI + Desktop)

```bash
# Run the CLI
cargo run -p savhub -- apply

# Run the desktop app
cargo run -p savhub-desktop
```

### Docker Compose

```bash
docker compose up
```

### Just Commands

A [justfile](https://just.systems) is included:

```bash
just setup           # Initialize (env + db)
just backend         # Run backend
just frontend        # Run frontend dev server
just cli apply       # Run CLI command
just desktop         # Run desktop app (debug)
just check           # Check workspace
just test            # Test workspace
just fmt             # Format code
just ci              # Run all quality gates
```

### GitHub OAuth Setup

Create a GitHub OAuth app with:
- Homepage URL: `http://127.0.0.1:5007`
- Authorization callback URL: `http://127.0.0.1:5006/api/v1/auth/github/callback`

## Documentation

Full documentation is available at [savhub.ai/docs](https://savhub.ai/en/docs).

## License

[Apache-2.0](LICENSE)
