# C1 — Split `service/index_jobs.rs` (2032 LOC)

## Problem
`index_jobs.rs` mixes five concerns in one file:
clone management, candidate discovery, AI metadata generation,
security scanning, and job persistence. Hard to review, hard to test.

## Target layout

```
service/index_jobs/
├── mod.rs          // public re-exports + submit_index_job, list, get
├── clone.rs        // checkout, fetch, sha tracking, reused detection
├── discover.rs     // SKILL.md walking, group plans, path_to_display_name
├── metadata.rs     // AI flock/repo metadata generation + ai_request_cache
├── scan.rs         // security_scan glue + verdict persistence
└── persist.rs      // upsert flock/skill rows, soft delete stale skills
```

`index_jobs.rs` becomes `mod.rs` orchestrator (~300 LOC).

## Plan
1. Extract pure helpers first (no DB): `discover.rs` + tests.
2. Extract `clone.rs` (touches `git_ops`).
3. Extract `metadata.rs` (touches `ai`, `ai_request_cache`).
4. Extract `scan.rs` (touches `security_scan`, `llm_eval`).
5. Extract `persist.rs` (Diesel writes).
6. `mod.rs` becomes a thin `submit_index_job` that calls the above in order.

## Constraints
- **No behavior changes.** Pure move + visibility tweaks.
- Each step is its own commit so `git blame` survives via `--follow`.
- Existing 21 tests in `service::index_jobs::tests` must pass after each step.

## Test plan
`cargo test -p server --lib service::index_jobs::` after every commit.
