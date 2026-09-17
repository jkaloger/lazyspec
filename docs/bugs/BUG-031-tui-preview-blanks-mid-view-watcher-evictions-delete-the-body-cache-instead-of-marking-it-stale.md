---
title: 'TUI preview blanks mid-view: watcher evictions delete the body cache instead of marking it stale'
type: bug
status: fixed
author: Jack Kaloger
date: 2026-09-17
tags: []
related:
- related-to: BUG-030
reviewed: 6a6597afadd513fdc725c085a8cd8ab5cd45f1c8
---

## Expected

Viewing a document in the TUI while a background poll runs keeps the rendered body on screen. A re-fetch or re-expansion swaps the body when the new one is ready; it never blanks the panel in between.

## Actual

The preview blanks for a frame or more and the body comes back. Worst on git-ref docs, where the background poll rewrites the cache file under `.lazyspec/cache/<type>/`; a filesystem doc only flickers when its file is actually edited. Docs with `@ref` directives blank for longer, because the on-disk expansion cache is wiped at the same moment and the body must be re-expanded from scratch.

## Cause

The preview renders the body as `expanded_body_cache.get(&doc.path).cloned().unwrap_or_default()` (`src/tui/views/panels.rs:1211` and `:1414`), so any cache eviction paints an empty document until the background expansion thread returns. The `FileChange` arm (`src/tui/infra/event_loop.rs:546-565`) evicts on every watcher event:

- `.md` events call `expanded_body_cache.remove(key)` for the doc on screen;
- atomic-write temp paths have no extension, so `has_non_md` is set and the arm calls `expanded_body_cache.clear()` **and** `disk_cache.clear()`, deleting every file in `~/.lazyspec/cache`.

Reproduced with a real `notify` watcher over a doc dir and one `atomic_write` of `DOC-1.md` — the exact call `fetch_git_ref` makes (`src/engine/sync.rs:609`):

```
Create(File)       .tmpVPV59x   ext=None
Modify(Name(Any))  .tmpVPV59x   ext=None
Modify(Metadata)   .tmpVPV59x   ext=None
Create/Modify x4   DOC-1.md     ext=Some("md")
```

One doc write, three extensionless events. BUG-030 fixed the validation storm the same events caused and listed the temp-path classification as optional; the blanking behaviour was never addressed.

## Fix

1. Stale-while-revalidate the body. Invalidation marks a path stale (`expansion_stale` set) instead of deleting its cache entry; `request_expansion` re-dispatches on staleness and `ExpansionResult` overwrites in place. The previous body stays on screen until the new one lands.
2. Ignore atomic-write temp paths in the `FileChange` handler (skip event paths whose file name starts with `.`), so a poll no longer sets `has_non_md` and no longer wipes `~/.lazyspec/cache`. This is BUG-030's deferred item 3.

Fix 1 removes the flicker; fix 2 removes the cache destruction that makes each re-expansion slow.
