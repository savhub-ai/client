# B1 — Hash bearer tokens at rest

## Problem
`user_tokens.token` currently stores the raw bearer token. A DB read leak
exposes every active session. Lookup is by exact match (`token.eq(...)`).

## Goal
Persist only `SHA-256(token)` plus a short non-secret `prefix` for UI display.
Tokens at rest become unusable to an attacker.

## Plan

1. **Migration** `add-token-hash-to-user-tokens`
   - Add columns: `token_hash TEXT`, `token_prefix TEXT`.
   - Backfill `token_hash = sha256(token)`, `token_prefix = substr(token, 1, 8)`.
   - Add unique index on `token_hash`.
   - Drop `UNIQUE` from `token`; in a follow-up migration drop the `token` column entirely (two-phase to allow rollback).

2. **`auth.rs`**
   - Lookup: `user_tokens::token_hash.eq(sha256_hex(presented))`.
   - Constant-time compare on the row id (already provided by indexed lookup).

3. **`github_auth.rs::issue_token`**
   - Generate token, store `(hash, prefix)`, return raw token to client **once**.

4. **CLI / desktop**
   - No change. Client already stores raw token in keyring/config; only the
     server side hashes for storage.

5. **Admin revoke flow**
   - `admin.rs:276` already deletes by `user_id` — unchanged.

## Compatibility
**Breaking for any cached server-side lookups**, but transparent to clients
(they already hold the raw token). No CLI release required.

## Test plan
- Unit: `hash_token_is_hex_64`, `lookup_by_hash_succeeds`, `lookup_by_raw_fails_after_migration`.
- Integration: issue → use → revoke roundtrip against test DB.

## Rollout
1. Ship migration + dual-read code (accept either `token` or `token_hash` lookup).
2. Backfill in production.
3. Ship follow-up: drop `token` column, single-path lookup.
