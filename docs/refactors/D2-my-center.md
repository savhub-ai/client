# D2 — "My" center (My Skills / Stars / History)

## Problem
Authenticated users have no aggregate view of their own activity. They must
browse to individual repos/skills to find their stars or comments.

## Scope
Three new pages under `/me/...`:

| Route             | Shows                                              |
|-------------------|----------------------------------------------------|
| `/me/skills`      | Skills the user has published (via repos they own) |
| `/me/stars`       | Skills + flocks the user has starred               |
| `/me/history`     | Recent browse history (last 100)                   |

A `/me` index page links to all three plus existing settings stubs.

## Backend
APIs already exist:
- `GET /api/v1/skills?owner_id=...` (filter to add)
- `GET /api/v1/me/stars` **(new)** — returns starred skills + flocks
- `GET /api/v1/me/history` (already exists in browse_history)

## Frontend
- New file: `server/frontend/src/pages/me.rs`
- Add routes in `app.rs::Route` enum.
- Reuse existing `SkillCard` / `FlockCard` components.
- i18n keys: `me.title`, `me.skills`, `me.stars`, `me.history`, `me.empty.*`.

## Test plan
- Unauth → 401 redirect to login.
- Empty states render correctly.
- Pagination respects existing `limit` cap.
