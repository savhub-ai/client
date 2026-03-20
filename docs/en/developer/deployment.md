---
title: Deployment
description: Environment variables and server deployment
---

# Deployment

All configuration is via environment variables. Copy `.env.example` to `.env` (or `.env.local`) and fill in values.

## Required Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `DATABASE_URL` | PostgreSQL connection string | `postgres://postgres:postgres@127.0.0.1:55432/savhub_dev` |
| `SAVHUB_GITHUB_CLIENT_ID` | GitHub OAuth app client ID | `Ov23li...` |
| `SAVHUB_GITHUB_CLIENT_SECRET` | GitHub OAuth app secret | `70733a...` |
| `SAVHUB_GITHUB_REDIRECT_URL` | GitHub OAuth callback URL | `http://127.0.0.1:5006/api/v1/auth/github/callback` |

## Server Settings

| Variable | Description | Default |
|----------|-------------|---------|
| `SAVHUB_BIND` | Backend listen address | `127.0.0.1:5006` |
| `SAVHUB_FRONTEND_ORIGIN` | Frontend URL for CORS | `http://127.0.0.1:5007` |
| `SAVHUB_API_BASE` | Public API base URL | `http://{SAVHUB_BIND}/api/v1` |
| `SAVHUB_SPACE_PATH` | Data directory for registry checkout and repo caches | `./space` |

## User Roles

| Variable | Description |
|----------|-------------|
| `SAVHUB_GITHUB_ADMIN_LOGINS` | Comma-separated GitHub logins granted admin role on first login |
| `SAVHUB_GITHUB_MODERATOR_LOGINS` | Comma-separated GitHub logins granted moderator role on first login |

## Background Worker

| Variable | Description | Default |
|----------|-------------|---------|
| `SAVHUB_SYNC_INTERVAL_SECS` | Background flock sync interval | `300` |
| `SAVHUB_SYNC_STALE_HOURS` | Hours before a flock is considered stale for re-sync | `6` |
| `SAVHUB_AUTO_INDEX_MIN_INTERVAL_SECS` | Minimum interval between auto-index checks per repo | `3600` |

## GitHub OAuth Setup

Create a GitHub OAuth app with:

- **Homepage URL**: `http://127.0.0.1:5007`
- **Authorization callback URL**: `http://127.0.0.1:5006/api/v1/auth/github/callback`

## Docker Compose

```bash
# Set required env vars
export SAVHUB_REGISTRY_GIT_TOKEN=ghp_xxx

# Start everything
docker compose up
```
