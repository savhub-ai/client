# Savhub Client

The official client toolset for [Savhub](https://savhub.ai) — an open registry for AI skills (`SKILL.md`).

Savhub Client automatically detects your project type, resolves the right AI skills from the registry, and syncs them to your AI coding agents.

## Components

| Crate | Binary | Description |
|---|---|---|
| `cli` | `savhub` | Command-line interface |
| `desktop` | `savhub-desktop` | Desktop GUI (Dioxus) |
| `shared` | *(library)* | Shared logic: selectors, registry cache, client detection |

## Supported AI Agents

Skills are synced to whichever agents are installed on your machine:

- Claude Code
- Codex
- Cursor
- Windsurf
- Continue
- VS Code (Copilot)

## Quick Start

### Install

Download the latest release from [savhub.ai](https://savhub.ai) or build from source (see below).

### Apply Skills to a Project

```bash
cd your-project
savhub
```

Running `savhub` with no arguments is equivalent to `savhub apply` — it detects your project type via selectors, resolves matching skills and flocks from the registry, and syncs them to your AI agents.

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
| `savhub install <skill>` | Install a skill by cloning its source repo |
| `savhub update` | Update installed skills |
| `savhub uninstall <skill>` | Remove a skill |
| `savhub list` | List installed skills in the current project |
| `savhub explore` | Browse skills from the registry API |
| `savhub inspect <skill>` | View detailed skill info |
| `savhub login` | Login via GitHub OAuth |
| `savhub logout` | Clear local auth token |
| `savhub whoami` | Show current authenticated user |

### Selectors

Selectors are rules that detect project types (e.g. "Cargo.toml exists" = Rust project). Built-in selectors cover Rust, Python, Go, Java, and frameworks like Salvo, Dioxus, Makepad, React, Vue, Angular, Next.js, and more.

```bash
savhub selector list        # List all selectors
savhub selector test        # Test selectors against current directory
savhub selector show <name> # Show selector details
```

### Flocks

Flocks are curated collections of skills grouped by topic or framework.

```bash
savhub flock list             # List available flocks
savhub flock show <slug>      # Show flock details and skills
savhub flock install <slug>   # Install all skills from a flock
```

### Registry Cache

The registry is cached locally from GitHub for offline access.

```bash
savhub registry sync   # Force sync the local cache
savhub registry info   # Show sync status
savhub registry search # Search cached skills
savhub registry list   # List cached skills
```

## Building from Source

Requires **Rust 1.94+**.

```bash
# Build everything
cargo build --workspace

# Run the CLI
cargo run -p savhub -- apply

# Run the desktop app
cargo run -p savhub-desktop

```

A [justfile](https://just.systems) is included for common tasks:

```bash
just build           # Build the full workspace
just cli apply       # Run a CLI command
just desktop         # Run the desktop app (debug)
just desktop-release # Run the desktop app (release)
just check           # Check compilation
just lint            # Run clippy
just fmt             # Format code
```

## Configuration

Global config is stored at `~/.config/savhub/config.toml` (or the OS-appropriate config directory):

```toml
registry = "https://savhub.ai/api/v1"
token = "your-auth-token"
language = "en"
workdir = "~/.savhub"
agents = ["claude-code", "cursor"]
```

Environment variables:

- `SAVHUB_REGISTRY` — override the registry API base URL
- `SAVHUB_CONFIG_PATH` — override the config file path

## Documentation

Full documentation is available at [savhub.ai/docs](https://savhub.ai/docs/en/client).

## License

Apache-2.0
