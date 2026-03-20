---
title: Registry Git Access
description: Configure push access to the registry repository
---

# Registry Git Access

The backend maintains a local checkout of the registry git repo. It commits index data (JSON files) after each scan and pushes to remote. Choose **one** authentication method.

## Option A: HTTPS Token (Recommended)

Best for GitHub repos. The token is embedded in the remote URL automatically.

| Variable | Description |
|----------|-------------|
| `SAVHUB_REGISTRY_GIT_URL` | Registry repo URL. Default: `https://github.com/savhub-ai/registry.git` |
| `SAVHUB_REGISTRY_GIT_TOKEN` | GitHub Personal Access Token |

### Generate a Token

1. Go to GitHub -> Settings -> Developer settings -> Personal access tokens -> Fine-grained tokens
2. Select the registry repository
3. Grant permission: **Contents** (Read and write)
4. Copy the token and set it as `SAVHUB_REGISTRY_GIT_TOKEN`

The token is embedded as `https://x-access-token:{token}@github.com/...`.

## Option B: SSH Key

Best for self-hosted git servers or when you prefer SSH.

| Variable | Description |
|----------|-------------|
| `SAVHUB_REGISTRY_GIT_URL` | SSH URL, e.g. `git@github.com:savhub-ai/registry.git` |
| `SAVHUB_REGISTRY_GIT_SSH_KEY` | Base64-encoded SSH private key |

### Encode Your Key

```bash
# Linux / macOS
base64 -w0 < ~/.ssh/id_ed25519

# Windows PowerShell
[Convert]::ToBase64String([IO.File]::ReadAllBytes("$HOME\.ssh\id_ed25519"))
```

The key is decoded at runtime, written to a temporary file with `0600` permissions (Unix), and used via `GIT_SSH_COMMAND`.

## Option C: No Credentials

If neither token nor SSH key is set, the backend relies on the system git credential helper. This works when the machine already has push access configured (e.g. via `gh auth` or a global credential store).

## How It Works

1. On startup, the backend clones (or pulls) the registry repo to `{SAVHUB_SPACE_PATH}/registry/`
2. After each index job completes, changed files are staged and committed
3. A single `git push` sends changes to the remote
4. All registry writes are serialized via a process-wide lock to prevent conflicts
5. Git identity is configured as `savhub-bot <aston@sonc.ai>`
