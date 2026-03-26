---
title: Git Status Caching Strategy
type: adr
status: accepted
author: agent
date: 2026-03-23
tags: []
related:
- related-to: RFC-031
---




## Summary

We will use event-driven cache invalidation for git status queries in the git-signs gutter feature.

## Decision

Run `git status --porcelain` once on startup and cache the result. Invalidate and re-query when a TUI event changes files (document create, save, delete). Between invalidation events, reuse the cached result across all renders.

No background thread. No per-frame queries. The cache is a simple `HashMap<PathBuf, GitFileStatus>` owned by the app state, refreshed synchronously on invalidation.

## Options Considered

1. Query git on every frame render
   - Always current, but blocks the TUI on every render. Not viable.

2. Background thread with continuous polling
   - Non-blocking, but adds concurrency complexity (`Arc<Mutex<...>>`) for marginal benefit. The repo sizes lazyspec targets don't warrant this.

3. Event-driven invalidation (chosen)
   - Query once, cache until something changes. Simple, correct, and fast enough for the target use case. A background thread can be added later if profiling shows synchronous queries cause lag.

## Consequences

Positive: simple implementation, no concurrency primitives, always fresh after user actions.

Negative: cache may be stale if files change outside the TUI (e.g. another terminal). Acceptable for now; a file watcher or manual refresh keybind can address this later if needed.
