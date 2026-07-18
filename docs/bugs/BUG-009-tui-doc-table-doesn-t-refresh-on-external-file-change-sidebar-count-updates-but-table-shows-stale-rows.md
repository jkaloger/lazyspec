---
title: TUI doc table doesn't refresh on external file change; sidebar count updates but table shows stale rows
type: bug
status: triaged
author: unknown
date: 2026-07-18
tags: []
related:
- related-to: STORY-138
---

## Context

The TUI document table (types view and filters view) does not refresh its rows when a document is created or updated externally while the TUI is running. The per-type count in the types sidebar increments correctly, but the table keeps showing the previous rows/content.

## Root Cause

Two render paths read from different sources:

- **Sidebar count** — `App::doc_count` (`src/tui/state/app.rs:2123`) calls `store.list()` directly, so it reflects the store immediately.
- **Table rows** — `App::filtered_docs` (`src/tui/state/expansion.rs:143`) memoizes results into `filtered_docs_cache`, rebuilt only when the cache is `None`.

The `notify` file-watch handler `handle_app_event`'s `FileChange` arm (`src/tui/infra/event_loop.rs:510-539`) reloads the store via `store.reload_file` but never invalidates `filtered_docs_cache` nor calls `rebuild_search_index()`. Sibling arms that mutate the store do both: `CacheRefresh` (`event_loop.rs:563-564`) and `GhPushResult` (`event_loop.rs:575`). So after an external md change, the store updates (sidebar live) while the table serves stale cached paths.

## Expected vs Actual

- **Expected:** external create/update of a doc refreshes both the sidebar count and the table rows.
- **Actual:** sidebar count updates; table shows old content until an action that already nulls `filtered_docs_cache` (filter change, navigation reset, sync).

## Repro

1. Run the TUI (`cargo run`) in a lazyspec repo, types view.
2. From another shell, edit/create a doc under a watched type dir (e.g. `lazyspec update <ID> --body ...` or add a new doc).
3. Observe: sidebar type count increments; table still shows pre-change rows.

## Fix Direction

In the `FileChange` arm, after the reload loop / `refresh_validation`, invalidate the derived caches as the sibling arms do:

```rust
app.filtered_docs_cache = None;
app.rebuild_search_index();
```

Verify no regression to the web view and CLI (both re-read the store per request, so unaffected). Add a test around the FileChange handler asserting `filtered_docs_cache` is `None` after a reload.

