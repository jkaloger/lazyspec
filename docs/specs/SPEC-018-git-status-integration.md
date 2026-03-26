---
title: "Git Status Integration"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: [tui, git]
related:
  - related-to: docs/stories/STORY-077-git-status-gutter-in-main-document-list.md
  - related-to: docs/stories/STORY-078-git-status-gutter-in-search-and-filtered-views.md
---

## Summary

The TUI displays a single-character gutter column at the left edge of every document list, showing the git working-tree status of each file. Green indicates a new or untracked file; yellow indicates a modified, renamed, or deleted file. The gutter appears in the main document tree, filtered views, and the search overlay. The feature is backed by a synchronous cache that shells out to `git status --porcelain` and uses a stale/refresh invalidation model to avoid redundant subprocess calls.

## Status Classification

@ref src/engine/git_status.rs#GitFileStatus

The `GitFileStatus` enum has two variants: `New` and `Modified`. There is no `Deleted` variant because deleted files do not appear in the document store and therefore have no row to annotate.

@ref src/engine/git_status.rs#parse_porcelain_line

`parse_porcelain_line` maps the two-character status codes from `git status --porcelain` output to a `GitFileStatus` and a `PathBuf`. Lines shorter than 4 bytes are rejected (the format is `XY <path>`). The classification rules are:

- `??`, `A `, `AM` map to `GitFileStatus::New` (untracked or staged-new files).
- `R `, `RM` map to `GitFileStatus::Modified`, using the destination path extracted from the `old -> new` rename format.
- All other codes (including `M `, ` M`, `MM`, `D `) fall through to `GitFileStatus::Modified`.

Renamed files use `rsplit_once(" -> ")` to extract the destination path. If the separator is missing, the entire raw path is used.

## Querying Git Status

@ref src/engine/git_status.rs#query_git_status

`query_git_status` spawns `git status --porcelain` with `current_dir` set to the repository root. If the command fails (non-zero exit or spawn error), it returns `None`. On success, it iterates each line of stdout through `parse_porcelain_line` and collects the results into a `HashMap<PathBuf, GitFileStatus>`. Paths in this map are relative to the repository root, matching the porcelain output format.

## Cache

@ref src/engine/git_status.rs#GitStatusCache

`GitStatusCache` holds an `Option<HashMap<PathBuf, GitFileStatus>>`, a `stale` flag, and the `repo_root` path. The `Option` is `None` when the cache was constructed outside a git repository (the `query_git_status` call in `new` returned `None`).

The cache follows an invalidate-then-refresh lifecycle. `invalidate()` sets the `stale` flag to `true`. `refresh()` checks the flag and, only if stale, re-runs `query_git_status` and clears the flag. If the cache is not stale, `refresh()` is a no-op.

`get()` performs a lookup against the stored map. It returns `None` both when the file has no git changes and when the cache itself is `None` (non-git-repo case).

## Cache Wiring

@ref src/tui/state/app.rs#App

The `App` struct owns a `git_status_cache: GitStatusCache` field, constructed with `GitStatusCache::new(store.root())` during `App::new`.

@ref src/tui/infra/event_loop.rs#handle_app_event

Cache invalidation happens in the event loop. When a `FileChange` event completes processing (after store reload and validation refresh), `git_status_cache.invalidate()` is called. The same invalidation occurs after a successful `CreateComplete` event. This means the cache becomes stale after any document is created or modified on disk.

@ref src/tui/views.rs#draw

The `draw` function calls `git_status_cache.refresh()` at the top of every render cycle. This ensures that if the cache was invalidated by a file change event, the next frame re-queries git before any view reads the cache. If the cache is not stale, the refresh is a no-op, so steady-state rendering does not spawn subprocesses.

## Gutter Rendering -- Document Tree

@ref src/tui/views/panels.rs#doc_table_widths

The document table layout includes a 1-character gutter column as the first element (`Constraint::Length(1)`), followed by the tree-indent, ID, title, status, and tags columns.

@ref src/tui/views/panels.rs#doc_row_for_node

Each row looks up `node.path` in `git_status_cache.get()`. `GitFileStatus::New` produces a green `┃` cell, `GitFileStatus::Modified` a yellow `┃` cell, and `None` a blank space. The gutter cell is prepended to the row before the tree-indent cell.

## Gutter Rendering -- Search Overlay

@ref src/tui/views/overlays.rs#draw_search_overlay

The search overlay renders each result as a `ListItem` containing a `Line` of spans. The first span is the gutter, derived from the same `git_status_cache.get(path)` lookup with identical color rules: green `┃` for `New`, yellow `┃` for `Modified`, space for `None`. This is a leading span in the `Line`, not a table column, because the search overlay uses a `List` widget rather than a `Table`.

## Gutter Rendering -- Filtered Views

The filtered document list in `draw_filters_mode` reuses the same `doc_table_widths` layout and the same gutter-cell construction as the main document tree. Each row calls `git_status_cache.get(&doc.path)` and prepends the gutter cell identically. No additional git commands are issued; all views read from the single shared cache.

## Non-Git-Repository Behavior

When the TUI opens a project that is not inside a git repository, `query_git_status` returns `None`, so `GitStatusCache.statuses` is `None`. Every `get()` call returns `None`, and every gutter cell renders as a blank space. No errors are raised.
