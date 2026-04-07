# D5 — Custom selector management UI

## Problem
Backend already exposes:
- `GET    /api/v1/me/selectors/custom`
- `POST   /api/v1/me/selectors/custom`
- `DELETE /api/v1/me/selectors/custom/{id}`

But the frontend has no page to create or edit them. Users can only manage
their selectors via raw API calls.

## Scope
New page `/me/selectors`:

1. **List** existing custom selectors with name, pattern summary, last used.
2. **Create form** with fields:
   - `name` (slug)
   - `kind` (file_glob | dependency | regex | composite)
   - `pattern` (textarea, monospace, with format hint per kind)
   - `agents[]` (multiselect: claude-code / cursor / windsurf / …)
3. **Edit** & **delete** actions.
4. **Validate** by calling `POST /selectors/dry-run` (server-side) — needs new
   endpoint that runs the selector against a synthetic project payload.

## Out of scope (follow-up)
- DSL editor with syntax highlighting.
- Live preview against the user's actual indexed repos.

## Test plan
- CRUD round-trip via TestClient.
- Validation errors surface inline on the form.
- i18n keys for all field labels.
