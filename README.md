# Savhub

Savhub is an open registry and package manager for AI skills (`SKILL.md`). Skills live in GitHub repos you already own. Savhub scans them, organizes them into flocks, and makes them discoverable and installable by AI coding agents.

> **Note:** Currently under development. Features may be incomplete, unstable, or subject to change.

## Components

| Crate | Binary | Description |
|---|---|---|
| `server` | `savhub-backend` | Backend API server (Salvo + Diesel + PostgreSQL) |
| `web` | *(WASM)* | Web frontend (Dioxus) |
| `shared` | *(library)* | Shared types between server and clients |
| `local` | *(library)* | Client-side logic: selectors, registry cache, client detection |
| `cli` | `savhub` | Command-line interface |
| `desktop` | `savhub-desktop` | Desktop GUI (Dioxus native) |

## Stack

| Layer | Technology |
|-------|-----------|
| Frontend | Dioxus 0.7 (WASM) |
| Backend | Salvo 0.89 |
| ORM | Diesel 2.3 |
| Database | PostgreSQL 18 |
| Registry | Git-based JSON index |
| Desktop | Dioxus native |
| CLI TUI | Ratatui |

## Architecture

```
User submits git URL
       |
  Index job created (parallel)
       |
  Clone/fetch git repo -> scan SKILL.md files
       |
  Match index_rules -> pick strategy (each_dir_as_flock / smart LCA)
       |
  Generate flock metadata (AI when available)
       |
  Persist to DB + write to registry git repo (serial, locked)
```

- **Repos**: Git repositories tracked by Savhub
- **Flocks**: Collections of related skills within a repo
- **Skills**: Individual `SKILL.md` files discovered by scanning

## Supported AI Agents

Skills are synced to whichever agents are installed on your machine:

- Claude Code
- Codex
- Cursor
- Windsurf
- Continue
- VS Code (Copilot)

## Quick Start

### Install the CLI

Download the latest release from [savhub.ai](https://savhub.ai) or use one of the install scripts:

**Linux / macOS:**
```sh
curl -fsSL https://raw.githubusercontent.com/savhub-ai/savhub/main/scripts/install.sh | bash
```

**Windows (PowerShell):**
```powershell
irm https://raw.githubusercontent.com/savhub-ai/savhub/main/scripts/install.ps1 | iex
```

### Apply Skills to a Project

```bash
cd your-project
savhub
```

Running `savhub` with no arguments detects your project type via selectors, resolves matching skills and flocks from the registry, and syncs them to your AI agents.

### Login

```bash
savhub login
```

Authenticates via GitHub OAuth so you can publish, star, and manage skills.

## CLI Commands

| Command | Description |
|---|---|
| `savhub` | Detect project & apply skills (default) |
| `savhub apply` | Same as above, with extra flags (`--dry-run`, `--yes`, `--agents`, etc.) |
| `savhub search <query>` | Search skills in the registry |
| `savhub fetch <skill>` | Fetch a skill by cloning its source repo |
| `savhub update` | Update fetched skills |
| `savhub prune <skill>` | Remove a skill |
| `savhub list` | List fetched skills in the current project |
| `savhub explore` | Browse skills from the registry API |
| `savhub inspect <skill>` | View detailed skill info |
| `savhub login` | Login via GitHub OAuth |
| `savhub logout` | Clear local auth token |
| `savhub whoami` | Show current authenticated user |

### Selectors

Selectors detect project types (e.g. "Cargo.toml exists" = Rust project). Built-in selectors cover Rust, Python, Go, Java, and frameworks like Salvo, Dioxus, Makepad, React, Vue, Angular, Next.js, and more.

```bash
savhub selector list        # List all selectors
savhub selector test        # Test selectors against current directory
savhub selector show <name> # Show selector details
```

### Flocks

```bash
savhub flock list             # List available flocks
savhub flock show <slug>      # Show flock details and skills
savhub flock fetch <slug>     # Fetch all skills from a flock
```

## Configuration

### Server

All server configuration is via environment variables. Copy `.env.example` to `.env` and fill in values.

#### Required

| Variable | Description | Example |
|----------|-------------|---------|
| `DATABASE_URL` | PostgreSQL connection string | `postgres://postgres:postgres@127.0.0.1:55432/savhub_dev` |
| `SAVHUB_GITHUB_CLIENT_ID` | GitHub OAuth app client ID | `Ov23li...` |
| `SAVHUB_GITHUB_CLIENT_SECRET` | GitHub OAuth app secret | `70733a...` |
| `SAVHUB_GITHUB_REDIRECT_URL` | GitHub OAuth callback URL | `http://127.0.0.1:5006/api/v1/auth/github/callback` |

#### Server Settings

| Variable | Description | Default |
|----------|-------------|---------|
| `SAVHUB_BIND` | Backend listen address | `127.0.0.1:5006` |
| `SAVHUB_FRONTEND_ORIGIN` | Frontend URL for CORS | `http://127.0.0.1:5007` |
| `SAVHUB_API_BASE` | Public API base URL | `http://{SAVHUB_BIND}/api/v1` |
| `SAVHUB_SPACE_PATH` | Data directory for registry checkout and repo caches | `./space` |

#### Registry Git Access

The backend maintains a local checkout of the registry git repo and pushes index data after each scan. Choose one authentication method:

**Option A: HTTPS Token** (recommended)

| Variable | Description |
|----------|-------------|
| `SAVHUB_REGISTRY_GIT_URL` | Registry repo URL (default: `https://github.com/savhub-ai/registry.git`) |
| `SAVHUB_REGISTRY_GIT_TOKEN` | GitHub PAT with `Contents: Read and write` on the registry repo |

**Option B: SSH Key**

| Variable | Description |
|----------|-------------|
| `SAVHUB_REGISTRY_GIT_URL` | SSH URL, e.g. `git@github.com:savhub-ai/registry.git` |
| `SAVHUB_REGISTRY_GIT_SSH_KEY` | Base64-encoded SSH private key |

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

### Client

Global config is stored at `~/.config/savhub/config.toml`:

```toml
registry = "https://savhub.ai/api/v1"
token = "your-auth-token"
language = "en"
workdir = "~/.savhub"
agents = ["claude-code", "cursor"]
```

Environment variables: `SAVHUB_REGISTRY`, `SAVHUB_CONFIG_PATH`.

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
cd web && dx serve --platform web --port 5007
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
export SAVHUB_REGISTRY_GIT_TOKEN=ghp_xxx
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

## API Endpoints

### Public

- `GET /api/v1/health`
- `GET /api/v1/search?q=...`
- `GET /api/v1/skills` / `GET /api/v1/skills/{slug}`
- `GET /api/v1/flocks` / `GET /api/v1/flocks/{id}`
- `GET /api/v1/repos` / `GET /api/v1/repos/{domain}/{path_slug}`
- `GET /api/v1/users` / `GET /api/v1/users/{handle}`
- `GET /api/v1/resolve?slug=...&hash=...`
- `GET /api/v1/download?slug=...`

### Authenticated

- `GET /api/v1/whoami`
- `POST /api/v1/index` / `GET /api/v1/index/list` / `GET /api/v1/index/{id}`
- `POST /api/v1/repos`
- `POST /api/v1/skills/{slug}/comments`
- `POST /api/v1/skills/{slug}/star`
- `POST /api/v1/repos/{domain}/{path_slug}/flocks/{flock_slug}/rate`
- `POST /api/v1/repos/{domain}/{path_slug}/flocks/{flock_slug}/star`

### Admin

- `GET /api/v1/management/summary`
- `GET/POST /api/v1/management/site-admins`
- `GET/POST /api/v1/management/index-rules`
- `POST /api/v1/management/index-rules/{id}`
- `DELETE /api/v1/management/index-rules/{id}`
- `POST /api/v1/management/users/{id}/role`
- `POST /api/v1/management/users/{id}/ban`

## Documentation

Full documentation is available at [savhub.ai/docs](https://savhub.ai/en/docs).

## License

[Apache-2.0](LICENSE)
