---
title: "TUI freezes after startup poll: cache rewrite storm runs full validation per watcher event"
type: bug
status: fixed
author: "Jack Kaloger"
date: 2026-09-14
tags: []
related: []
reviewed: d717d90219edb9f960d08f0f0ecf2f873a15f434
---

## Expected

TUI stays responsive through the startup poll. Fetch completes, `CacheRefresh` lands, list redraws. Frame time stays in the tens of milliseconds.

## Actual

TUI freezes the moment the startup poll completes and stays frozen for a minute or more. Keys pile up unhandled. Repro repo: `make-a-wish-replatform`, 245 docs across github-issues (85), git-ref (94), clickup-tasks (38) and filesystem stores.

Perf log from one run (`LAZYSPEC_LOG=1`): main loop steady at ~20ms/frame for 4s, receives one background event, never logs again. Input thread keeps reading keys for 28s until quit.

```
[    4042.598ms] between_frames: 0.003ms
[    4044.317ms] recv_wait: 1.699ms          <- last main-thread line
[    4169.799ms] input_thread: read key Char('h')
[   32242.661ms] input_thread: read key Char('c')
```

## Cause

Self-inflicted watcher storm. Four parts multiply:

1. **Watch set covers poll output.** `doc_root` (`src/engine/store.rs:123`) resolves remote-backed types to `.lazyspec/cache/<type>`, so `watch_paths` watches the dirs the poll writes into.
2. **Board reconcile rewrites every github-issues doc unconditionally.** `src/engine/sync.rs:717` calls `write_cache_file` for every loaded doc with no changed-content check. Sibling path `issue_cache.rs:327` has the check. Cache mtimes confirm: one poll rewrote all 38 bugs + 42 stories + 5 spikes.
3. **Atomic write = temp file + rename.** Two to three watcher events per doc. Temp path has no `.md` extension, so `has_non_md` also clears `expanded_body_cache` and `disk_cache` every poll.
4. **Handler validates per event on the UI thread.** `FileChange` arm (`src/tui/infra/event_loop.rs:509`) runs `refresh_validation` for each event. Full `validate_without_stale` on this store measures 0.24-0.34s (CLI `validate`, zero git spawns). The drain loop `while let Ok(event) = rx.try_recv()` cannot empty while validation is slower than events arrive.

~85 docs x 2-3 events x 0.3s = 50-75s per poll. Poll repeats every `cache_ttl` (60s default). UI effectively never idle.

Ruled out: store load (50ms, no git), git status (cached), diagram rendering (threaded, no d2 blocks in repo), editor pushes (threaded), gh store lock (try_lock), file locks (none in engine).

## Fix

1. Validate once per drained batch, not per event. `FileChange` handler sets a dirty flag; run loop calls `refresh_validation` once after the drain. Fixes the class.
2. `sync.rs:717`: skip write when rendered content matches existing file, mirroring `issue_cache.rs:327`. Removes the storm at source.
3. Optional: ignore atomic-write temp paths in the handler so a poll no longer clears body caches.

Not: dropping cache dirs from the watch set. CLI writes from another process would go unseen until next poll.
