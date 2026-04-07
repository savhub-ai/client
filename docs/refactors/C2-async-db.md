# C2 — Stop blocking the Tokio runtime on Diesel calls

## Problem
Every handler does `let mut conn = db_conn()?;` then runs **synchronous**
Diesel queries inside an async context. Under load this starves the Tokio
worker pool: a slow query blocks an entire worker thread.

## Two viable approaches

### Option A — `tokio::task::spawn_blocking` wrapper
- **Pros**: minimal dependency churn; works with current Diesel sync API.
- **Cons**: every handler grows a closure; `&mut conn` lifetimes get awkward;
  r2d2 pool size must be tuned against blocking-thread pool.

### Option B — migrate to `diesel-async` + `bb8`
- **Pros**: idiomatic; no closure boilerplate; native async cancellation.
- **Cons**: requires Diesel 2.x compatible `diesel-async ^0.5`; some query
  builder differences; ~1 day of mechanical edits.

## Recommendation
**Option B.** The codebase is already heavily async (Salvo + reqwest +
tokio::process). Half-async DB calls are technical debt that compounds.

## Plan (Option B)
1. Add `diesel-async = { version = "0.5", features = ["postgres", "bb8"] }`.
2. Replace `r2d2::Pool<ConnectionManager<PgConnection>>` with
   `bb8::Pool<AsyncPgConnection>` in `db.rs`.
3. Mechanical: `conn` becomes `&mut AsyncPgConnection`; every `.execute(&mut conn)?`
   becomes `.execute(&mut conn).await?`.
4. Run migrations remain sync (use a one-shot blocking conn at startup).
5. Smoke test all routes against staging DB.

## Risk
Touches every service file. Land this in a single PR (not piecemeal) to keep
the build green.

## Test plan
- Existing tests must still compile + pass.
- Add a load test: 200 concurrent `/api/v1/skills` requests should not
  exhaust workers (verify with `tokio-console`).
