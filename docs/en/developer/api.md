---
title: API Reference
description: REST API endpoints
---

# API Reference

All endpoints are under `/api/v1/`.

## Public Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Health check |
| GET | `/search?q=...` | Full-text search across skills |
| GET | `/skills` | List skills (supports `sort`, `limit`, `cursor`, `q`) |
| GET | `/skills/{slug}` | Skill detail |
| GET | `/skills/{slug}/file?path=...` | Get a file from a skill version |
| GET | `/flocks` | List all flocks |
| GET | `/flocks/{id}` | Flock detail by UUID |
| GET | `/repos` | List repositories |
| GET | `/repos/{domain}/{path_slug}` | Repository detail with flocks and skills |
| GET | `/users` | List users |
| GET | `/users/{handle}` | User profile |
| GET | `/resolve?slug=...&hash=...` | Resolve skill by fingerprint |
| GET | `/download?slug=...` | Download skill as zip bundle |

## Authenticated Endpoints

Require `Authorization: Bearer {token}` header.

| Method | Path | Description |
|--------|------|-------------|
| GET | `/whoami` | Current user info |
| POST | `/index` | Submit an index job |
| GET | `/index/list` | List index jobs |
| GET | `/index/{id}` | Get index job status |
| POST | `/repos` | Create a repository |
| POST | `/skills/{slug}/comments` | Add a comment |
| DELETE | `/skills/{slug}/comments/{id}` | Delete a comment |
| POST | `/skills/{slug}/star` | Toggle star on a skill |
| POST | `/repos/{d}/{p}/flocks/{s}/comments` | Add flock comment |
| POST | `/repos/{d}/{p}/flocks/{s}/rate` | Rate a flock |
| POST | `/repos/{d}/{p}/flocks/{s}/star` | Toggle star on a flock |
| POST | `/repos/{d}/{p}/flocks/{s}/block` | Block a flock |
| DELETE | `/repos/{d}/{p}/flocks/{s}/block` | Unblock a flock |
| GET | `/blocks/flocks` | List blocked flocks |
| POST | `/history` | Record a view |
| GET | `/history` | Get browse history |
| POST | `/reports` | Create a report |
| GET | `/reports` | List reports (staff) |
| POST | `/reports/{id}/review` | Review a report (staff) |

## Admin Endpoints

Require admin role.

| Method | Path | Description |
|--------|------|-------------|
| GET | `/management/summary` | Dashboard counts and audit log |
| GET | `/management/site-admins` | List site admins |
| POST | `/management/site-admins` | Add site admin |
| DELETE | `/management/site-admins/{id}` | Remove site admin |
| GET | `/management/index-rules` | List index rules |
| POST | `/management/index-rules` | Create index rule |
| POST | `/management/index-rules/{id}` | Update index rule |
| DELETE | `/management/index-rules/{id}` | Delete index rule |
| POST | `/management/users/{id}/role` | Set user role |
| POST | `/management/users/{id}/ban` | Ban user |
| DELETE | `/skills/{slug}` | Soft-delete a skill |
| POST | `/skills/{slug}/restore` | Restore a deleted skill |
| POST | `/skills/{slug}/moderation` | Update moderation status |

## WebSocket

Connect to `/api/v1/ws` for real-time index job progress.

```json
// Subscribe to a job
{"action": "subscribe", "job_id": "019..."}

// Receive progress events
{"type": "index_progress", "job_id": "019...", "status": "running",
 "progress_pct": 50, "progress_message": "Scanning for skills..."}

// Unsubscribe
{"action": "unsubscribe", "job_id": "019..."}
```

## Authentication

Savhub uses GitHub OAuth. The flow:

1. Frontend redirects to `GET /auth/github/start`
2. GitHub redirects back to `GET /auth/github/callback`
3. Backend creates a session token and redirects to frontend with token
4. Frontend stores token in localStorage and sends as `Authorization: Bearer {token}`
