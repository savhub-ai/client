# Savhub CLI

Command-line tool for managing AI skills, project configuration, and the Savhub registry.

## Installation

```bash
cargo build --release --package savhub
# Binary: target/release/savhub (or savhub.exe on Windows)
```

## Global Options

| Flag | Description |
|---|---|
| `--workdir <path>` | Project directory (default: current directory) |
| `--dir <path>` | Skills sub-directory within workdir (default: `skills`) |
| `--site <url>` | API site URL |
| `--registry <url>` | Registry URL |
| `--no-input` | Disable interactive prompts |

## Commands

### `savhub auto` — Auto-configure project

Run all selectors against the current directory. Matched selectors contribute presets and skills which are applied to the project. Manually added skills are never removed.

```bash
savhub auto              # Detect and apply (with confirmation)
savhub auto --dry-run    # Preview changes without applying
savhub auto --yes        # Skip confirmation prompt
```

### `savhub selector` — Manage selectors

```bash
savhub selector list             # List all configured selectors
savhub selector show "Rust"      # Show details of a selector (partial name match)
savhub selector test             # Run selectors against current dir (no changes)
```

### `savhub registry` — Registry cache

Registry metadata is fetched live from the configured Savhub REST API.

```bash
savhub registry search "rust web"            # Search skills
savhub registry search "frontend" --limit 50
savhub registry list                         # List with pagination
savhub registry list --page 2 --page-size 50
savhub registry list --query "python" --status active --json
```

### `savhub login` / `savhub logout` / `savhub whoami` — Authentication

```bash
savhub login                # GitHub OAuth (opens browser)
savhub login --no-browser   # Print URL instead
savhub whoami               # Show current user
savhub logout               # Clear token
```

These are also available as `savhub auth login`, `savhub auth logout`, `savhub auth whoami`.

### `savhub search` — Search registry skills

```bash
savhub search "code review"
savhub search rust --limit 50
```

### `savhub explore` — Browse skills

```bash
savhub explore                        # Browse latest skills
savhub explore --limit 50 --sort name
savhub explore --json                 # JSON output
```

### `savhub inspect` — Skill details

```bash
savhub inspect my-skill              # View skill info
savhub inspect my-skill --versions   # List versions
savhub inspect my-skill --files      # List files
savhub inspect my-skill --file SKILL.md  # View file content
savhub inspect my-skill --json       # JSON output
```

### `savhub fetch` — Fetch a skill

Clones the skill's source git repository (shallow, depth 1) into `~/.savhub/repos/` and marks the skill as fetched.

```bash
savhub fetch my-skill
savhub fetch my-skill --version 1.2.0
savhub fetch my-skill --force
```

### `savhub update` — Update project skills

Compares `savhub.lock` against `~/.savhub/fetched.json` and copies updated skill folders from the local repo cache. No network calls — run `savhub fetched --update` first to pull the latest from the registry.

```bash
savhub update
```

### `savhub fetched` — Manage fetched skills

```bash
savhub fetched               # List all fetched skills (from ~/.savhub/fetched.json)
savhub fetched --update      # Update all fetched repos/skills to latest
savhub fetched --update --force  # Force update even if already at latest
savhub fetched --prune       # Remove skills/repos not used by any project
```

### `savhub prune` — Remove a skill

```bash
savhub prune my-skill
savhub prune my-skill --yes  # Skip confirmation
```

### `savhub list` — List fetched skills

```bash
savhub list
```

### `savhub enable` / `savhub disable` — Project skills

```bash
savhub enable my-skill --repo /path/to/repo
savhub disable my-skill
```

### `savhub preset` — Manage presets

```bash
savhub preset create rust-dev --description "Rust tools"
savhub preset list
savhub preset show rust-dev
savhub preset add rust-dev cargo-audit clippy
savhub preset remove rust-dev clippy
savhub preset bind rust-dev    # Bind to current project
savhub preset unbind
savhub preset status
savhub preset delete rust-dev
```

Also available as `savhub profile ...`.

### `savhub star` / `savhub unstar` — Social

```bash
savhub star my-skill
savhub unstar my-skill
```

### `savhub transfer` — Ownership transfer

```bash
savhub transfer request my-skill new-owner
savhub transfer list
savhub transfer accept my-skill
savhub transfer reject my-skill
savhub transfer cancel my-skill
```

### `savhub delete` — Delete a skill (admin)

```bash
savhub delete my-skill
```

### `savhub mcp` — MCP server

```bash
savhub mcp register              # Register with AI clients
savhub mcp register --client "Claude Code"
savhub mcp unregister
savhub mcp status
savhub mcp serve                 # Start MCP server
```

## Configuration

| File | Location | Purpose |
|---|---|---|
| `config.toml` | `~/.savhub/` | User overrides (`[rest_api] base_url`) |
| `config.json` | `~/.savhub/` | Global settings (token, language) |
| `projects.json` | `~/.savhub/` | Known project directories |
| `profiles.json` | `~/.savhub/` | Preset definitions |
| `selectors.json` | `~/.savhub/` | Selector rules |
| `fetched.json` | `~/.savhub/` | Fetched skill tracking (lockfile) |
| `savhub.toml` | `<project>/` | Project presets and skills |
| `savhub.lock` | `<project>/` | Locked skill versions |
