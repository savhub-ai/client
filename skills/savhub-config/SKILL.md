---
name: savhub-config
description: Manage Savhub selectors and configuration that detect project types and fetch the right AI skills.
---

# savhub-config

You are an AI assistant with access to the **savhub** skill management system. You can create, edit, and manage **selectors** — rules that automatically detect project types and fetch the right AI skills.

## Config Locations

- **Global config**: `~/.savhub/config.toml`
- **Selectors**: `~/.savhub/selectors.json`
- **Project config**: `<project>/savhub.toml`
- **Lock file**: `<project>/savhub.lock`
- **Registry data**: fetched live from the configured Savhub registry API

## Selectors

Selectors are project type detection rules. When a user runs `savhub apply`, selectors scan the project directory and automatically fetch matching skills/flocks.

### Selector JSON Schema

```json
{
  "sign": "unique-id-string",
  "name": "Human Readable Name",
  "description": "What this selector detects",
  "folder_scope": ".",
  "rules": [ /* array of SelectorRule */ ],
  "match_mode": "all_match",
  "custom_expression": "",
  "skills": ["skill-slug"],
  "flocks": ["github.com/org/repo/flock-path"],
  "repos": ["github.com/org/repo"],
  "priority": 20,
  "enabled": true
}
```

### What are skills, flocks, and repos?

- **skill**: A single AI skill (e.g. `salvo-auth`). Referenced by **slug** (short name) in the `skills` array.
- **flock**: A collection of skills grouped together (e.g. `github.com/salvo-rs/salvo-skills/salvo-skills`). Referenced by **sign** (full path) in the `flocks` array.
- **repo**: A git repository containing one or more flocks (e.g. `github.com/org/repo`). Referenced by **sign** (domain/owner/repo) in the `repos` array. When a repo is added, ALL flocks from that repo are fetched.

### Rule Types

Each rule is a JSON object with a `"kind"` field:

| kind | fields | description |
|------|--------|-------------|
| `file_exists` | `path` | File exists relative to folder_scope |
| `folder_exists` | `path` | Directory exists relative to folder_scope |
| `glob_match` | `pattern` | Any file matching glob exists (supports `*`, `**`, `?`) |
| `file_contains` | `path`, `contains` | File contains substring (case-sensitive) |
| `file_regex` | `path`, `pattern` | File content matches regex |
| `env_var_set` | `name` | Environment variable is set and non-empty |
| `command_exits` | `command` | Shell command exits with code 0 |

### Match Modes

- `all_match` — All rules must be true (AND)
- `any_match` — At least one rule must be true (OR)
- `custom` — Use `custom_expression` with boolean logic: `(1 && 2) || !3` (1-based rule indices)

### Priority

Higher number = higher priority. When multiple selectors match and provide conflicting skills, higher priority wins.

- Language-level selectors: priority 10 (e.g., "Rust Project", "Python Project")
- Framework-level selectors: priority 20 (e.g., "Salvo", "React", "Next.js")
- Custom/specific selectors: priority 30+

## Resolving Short Names to Real Signs

Users will often refer to skills, flocks, or repos by short/partial names. You MUST resolve them to their full canonical identifiers before writing to `selectors.json`.

### How to search the registry

Use these CLI commands to find the real sign/slug:

```bash
# Search skills by keyword (returns slug, name, description)
savhub registry search <keyword>

# List all flocks (returns slug and sign)
savhub flock list

# Show a flock's details and its skills
savhub flock show <flock-slug>

# Search the remote API for skills
savhub search <keyword>
```

### Resolution rules

1. **Skills** — The `skills` array uses **slugs** (short names like `salvo-auth`).
   - If user says "add the salvo auth skill" → search: `savhub registry search salvo-auth` → use the `slug` from results.
   - If ambiguous, show the user all matches and ask which one.

2. **Flocks** — The `flocks` array uses **full sign** (like `github.com/salvo-rs/salvo-skills/salvo-skills`).
   - If user says "add salvo skills flock" → search: `savhub flock list` → find the matching flock → use its `sign` (NOT just the slug).
   - The sign format is always: `github.com/<owner>/<repo>/<flock-path>`

3. **Repos** — The `repos` array uses **repo sign** (like `github.com/org/repo`).
   - If user says "add all skills from makepad-skills" → resolve to `github.com/ZhangHanDong/makepad-skills`.
   - If user gives a URL like `https://github.com/org/repo` → normalize to `github.com/org/repo` (strip protocol, strip `.git`).

### Resolution examples

User says: "add salvo skills"
```bash
$ savhub flock list
  salvo-skills             5 skill(s)  Salvo Skills
$ savhub flock show salvo-skills
  Name:        Salvo Skills
  Slug:        salvo-skills
  Skills (5):
    - salvo-auth  Salvo Authentication
    - salvo-crud  Salvo CRUD
    ...
```
→ Flock sign: `github.com/salvo-rs/salvo-skills/salvo-skills` → add to `flocks` array.

User says: "add makepad repo"
```bash
$ savhub registry search makepad
  ... (shows results with repo info)
```
→ Repo sign: `github.com/ZhangHanDong/makepad-skills` → add to `repos` array.

User says: "add salvo-auth"
```bash
$ savhub registry search salvo-auth
  salvo-auth  Salvo Authentication Skill  ...
```
→ Skill slug: `salvo-auth` → add to `skills` array.

## How to Create a Selector

When the user describes a project type and what to fetch, follow these steps:

1. **Read** `~/.savhub/selectors.json` to see existing selectors
2. **Design** rules based on the project description:
   - Identify marker files (e.g., `Cargo.toml`, `package.json`, `go.mod`)
   - Identify distinguishing content (e.g., dependency names in manifest files)
   - Choose appropriate match mode
3. **Resolve** any skills/flocks/repos the user mentioned:
   - Run `savhub registry search`, `savhub flock list`, etc. to find the real sign/slug
   - NEVER guess a sign — always verify via CLI
   - If the search returns no results, verify the registry API is reachable
4. **Generate** the selector JSON with a unique sign (use format `det-{descriptive-name}`)
5. **Write** the updated selectors.json back
6. **Touch** `~/.savhub/.config-changed` to notify the desktop app
7. Tell the user to run `savhub apply` to activate

### Example: "When detect Salvo project, add salvo skills"

Step 1 — Resolve the flock:
```bash
savhub flock list | grep -i salvo
# → salvo-skills   5 skill(s)  Salvo Skills
savhub flock show salvo-skills
# → Sign: github.com/salvo-rs/salvo-skills/salvo-skills
```

Step 2 — Create selector:
```json
{
  "sign": "det-salvo",
  "name": "Salvo Web Framework",
  "description": "Detects Rust projects using Salvo and fetches Salvo skills.",
  "folder_scope": ".",
  "rules": [
    { "kind": "file_exists", "path": "Cargo.toml" },
    { "kind": "file_regex", "path": "Cargo.toml", "pattern": "salvo\\s*=" }
  ],
  "match_mode": "all_match",
  "custom_expression": "",
  "skills": [],
  "flocks": ["github.com/salvo-rs/salvo-skills/salvo-skills"],
  "repos": [],
  "priority": 20,
  "enabled": true
}
```

### Example: "When detect makepad, add all makepad skills"

Step 1 — Resolve: user said "all makepad skills" → use `repos` (adds all flocks from the repo):
```bash
savhub registry search makepad
# → find repo sign: github.com/ZhangHanDong/makepad-skills
```

Step 2 — Create selector:
```json
{
  "sign": "det-makepad",
  "name": "Makepad Project",
  "description": "Detects Makepad projects and fetches all Makepad skills.",
  "folder_scope": ".",
  "rules": [
    { "kind": "file_exists", "path": "Cargo.toml" },
    { "kind": "file_contains", "path": "Cargo.toml", "contains": "makepad" }
  ],
  "match_mode": "all_match",
  "custom_expression": "",
  "skills": [],
  "flocks": [],
  "repos": ["github.com/ZhangHanDong/makepad-skills"],
  "priority": 20,
  "enabled": true
}
```

### Example: "When detect FastAPI, add the fastapi-auth skill"

Step 1 — Resolve:
```bash
savhub registry search fastapi-auth
# → slug: fastapi-auth
```

Step 2 — Create selector:
```json
{
  "sign": "det-fastapi",
  "name": "FastAPI Project",
  "description": "Detects FastAPI projects and fetches auth skill.",
  "folder_scope": ".",
  "rules": [
    { "kind": "file_exists", "path": "pyproject.toml" },
    { "kind": "file_regex", "path": "pyproject.toml", "pattern": "fastapi" },
    { "kind": "file_exists", "path": "requirements.txt" },
    { "kind": "file_contains", "path": "requirements.txt", "contains": "fastapi" }
  ],
  "match_mode": "custom",
  "custom_expression": "(1 && 2) || (3 && 4)",
  "skills": ["fastapi-auth"],
  "flocks": [],
  "repos": [],
  "priority": 20,
  "enabled": true
}
```

### Example: "Detect monorepo with Rust backend and React frontend"

```json
{
  "sign": "det-fullstack-monorepo",
  "name": "Fullstack Monorepo (Rust + React)",
  "description": "Detects monorepos with a Rust backend and React frontend.",
  "folder_scope": ".",
  "rules": [
    { "kind": "file_exists", "path": "Cargo.toml" },
    { "kind": "file_exists", "path": "package.json" },
    { "kind": "file_regex", "path": "package.json", "pattern": "\"react\"\\s*:" },
    { "kind": "glob_match", "pattern": "**/src/main.rs" }
  ],
  "match_mode": "custom",
  "custom_expression": "1 && 2 && 3 && 4",
  "skills": [],
  "flocks": [],
  "repos": [],
  "priority": 30,
  "enabled": true
}
```

## How to Modify Selectors

To update an existing selector:
1. Read `~/.savhub/selectors.json`
2. Find the selector by `sign`
3. Modify the desired fields
4. Write the full file back
5. Touch `~/.savhub/.config-changed`

### Adding resources to an existing selector

When the user says "add X skill to the Y selector":
1. Resolve the skill/flock/repo short name to its real sign/slug (see Resolution rules above)
2. Read `selectors.json`, find the selector
3. Append to the appropriate array (`skills`, `flocks`, or `repos`)
4. Deduplicate — don't add if already present
5. Write back and touch signal file

## selectors.json File Format

The file wraps selectors in a versioned store:

```json
{
  "version": 1,
  "selectors": [
    { /* selector 1 */ },
    { /* selector 2 */ }
  ]
}
```

## Notification After Changes

After writing `selectors.json`, always touch the signal file so the desktop app can hot-reload:

```bash
# Unix/macOS
touch ~/.savhub/.config-changed

# Windows (PowerShell)
New-Item -Path "$env:USERPROFILE\.savhub\.config-changed" -ItemType File -Force

# Or use the CLI
savhub pilot notify
```

## Applying Changes

After creating or modifying selectors, instruct the user to run:

```bash
savhub apply
```

This will:
1. Sync the registry
2. Re-evaluate all selectors against the current project
3. Show which selectors matched
4. Fetch the corresponding skills
5. Write `savhub.lock` with fetched versions
