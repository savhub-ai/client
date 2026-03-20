---
title: Presets
description: Manage named skill groups
---

# Presets

Presets are named collections of skills and flocks that can be enabled together on projects. They provide a convenient way to group related skills.

## CLI Commands

```bash
# Create a preset
savhub preset create my-rust-tools --description "Rust development skills"

# List all presets
savhub preset list

# Show preset details
savhub preset show my-rust-tools

# Add skills to a preset
savhub preset add my-rust-tools rust-clippy rust-testing

# Remove skills from a preset
savhub preset remove my-rust-tools rust-testing

# Delete a preset
savhub preset delete my-rust-tools --yes

# Bind a preset to the current project
savhub preset bind my-rust-tools

# Unbind presets from the current project
savhub preset unbind

# Show preset binding status
savhub preset status
```

The `savhub preset` command is also available as `savhub profile` for backward compatibility.

## How Presets Work

### Creating Presets

A preset has a name, optional description, a list of skill slugs, and a list of flock slugs:

```bash
savhub preset create web-dev
savhub preset add web-dev react-patterns typescript-best-practices
```

### Binding to Projects

When you bind a preset to a project, all skills in that preset become available:

```bash
cd /path/to/my-project
savhub preset bind web-dev
```

This writes the binding to `savhub.toml`:

```toml
presets = ["web-dev"]
```

### Skill Resolution

Skills are resolved from multiple sources in this order:

1. **Explicit preset bindings** - Presets you manually bound to the project
2. **Selector-matched presets** - Presets contributed by matched selectors
3. **Manual skills** - Skills added individually via `savhub fetch`

### Including Flocks

Presets can reference flocks. When a preset is enabled, all skills in its flocks are also included:

```bash
# (Managed via desktop app or direct JSON editing)
```

## Storage

Presets are stored globally in `~/.config/savhub/profiles.json`. They are shared across all projects.

## Desktop App

The Savhub Desktop app provides a visual preset management UI on the **Presets** page, including:
- Create/edit/delete presets
- Search and add skills
- Bind/unbind presets to projects
