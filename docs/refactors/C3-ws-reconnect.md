# C3 — WebSocket heartbeat & client reconnect

## Problem
`/ws` streams index-job progress. The current frontend opens a single
WebSocket and never reconnects: refresh, sleep, or transient network drops
silently freeze the IndexPage progress UI.

## Server changes
1. Send a `Ping` frame every 20s; close idle connections after 60s of no pong.
2. On (re)connect, accept a `?since=<job_id>` query so the client can request
   a snapshot of any progress events emitted since its last seen offset.
3. Buffer the last N=200 events per job in memory (already partially done in
   `worker.rs`) and replay on subscribe.

## Client changes (Dioxus, `client/local` + frontend)
1. Wrap WebSocket in a small `ReconnectingSocket` helper:
   - Exponential backoff: 1s → 2s → 4s → … capped at 30s.
   - On open, send `{since: lastSeenJobId}` so the server can replay.
2. Visible "reconnecting…" badge on `IndexPage` when state ≠ Open.
3. Heartbeat: respond to server `Ping` with `Pong` (handled by browser, but
   verify the Salvo `WebSocket` upgrade enables it).

## Test plan
- Manual: `kill -STOP $(pidof server)` for 5s, confirm client reconnects.
- Unit: `ReconnectingSocket` backoff schedule under fake timers.
