---
title: GitHub Issues TUI Integration
type: iteration
status: draft
author: jkaloger
date: 2026-03-27
tags:
- tui
- github-issues
related:
- implements: STORY-098
---


## Goal

Wire the `github-issues` storage backend (RFC-037) into the TUI so that issues appear alongside filesystem and git-ref documents. Users edit issues through `$EDITOR`, cycle status via `s`, and see sync freshness at a glance.

## Task Breakdown

### Task 1: Unified document source in Store

Extend `Store::load_with_fs` to accept a `github-issues` document source alongside the existing filesystem loader. Documents returned from the GitHub cache layer get a `DocMeta` with a `source: GithubIssue` discriminant so downstream code can branch on provenance.

Files: `src/engine/store.rs`, `src/engine/store/loader.rs`, `src/engine/document.rs`

### Task 2: Document list rendering

GitHub Issues documents should appear in the type panel and doc list without special-casing the view layer. The panels code (`src/tui/views/panels.rs`) already iterates `Store::all_docs()`, so this task is mostly about ensuring the new source field renders a subtle `[gh]` badge in the ID column via `doc_row_cells`.

Files: `src/tui/views/panels.rs`

### Task 3: Edit flow with optimistic locking

When the user presses `e` on a GitHub Issues document:

1. Fetch the latest issue body from the cache (or network if stale).
2. Write to a temp file, recording the current `updated_at` ETag.
3. Launch `$EDITOR` via the existing `run_editor` path.
4. On return, parse frontmatter from the temp file.
5. Before pushing, compare the stored ETag against the remote. If it differs, show a conflict overlay warning the user.
6. On confirmation (or if no conflict), push the update via `gh` CLI or the HTTP backend.

The existing `editor_request` / `run_editor` / `input_paused` pattern in `event_loop.rs` handles the terminal suspend/restore lifecycle. The new logic wraps around this with pre-fetch and post-push steps.

Files: `src/tui/infra/event_loop.rs`, `src/tui/state/app.rs`, `src/tui/views/overlays.rs` (conflict dialog)

### Task 4: Status cycling

The `s` key already opens a `StatusPicker`. For GitHub Issues documents, the picker should present two options: `open` and `closed`. Selecting a status issues a `gh issue edit --state` call (or equivalent API request). Non-lifecycle documents fall through to the existing frontmatter rewrite path.

The branching point is `confirm_status_change` in `src/tui/state/app.rs`. Check `doc.source` and dispatch accordingly.

Files: `src/tui/state/app.rs`, `src/tui/views/keys.rs`

### Task 5: Sync indicator in status bar

Add a `last_sync: Option<Instant>` field to `App`. The background cache refresh (Task 6) updates this on each successful poll. The title bar line in `src/tui/views.rs` renders a right-aligned indicator:

- `synced 12s ago` in green when fresh
- `synced 2m ago` in yellow when approaching TTL
- `stale` in red when beyond 2x TTL

This reuses the same `outer[0]` layout slot as the mode indicator, placed to its left.

Files: `src/tui/views.rs`, `src/tui/state/app.rs`

### Task 6: Stale document warning badge

Documents whose cache age exceeds 2x TTL get a warning indicator in the doc list. Add a `is_stale` flag to the rendering path and display a `[!]` badge in the status column. The TTL threshold comes from the `github-issues` backend config.

Files: `src/tui/views/panels.rs`, `src/engine/cache.rs`

### Task 7: Background cache refresh

Add a new `AppEvent::CacheRefresh` variant. In the event loop, track a `next_poll: Instant`. When the loop iteration fires past `next_poll`, spawn a background thread that calls the cache refresh function from the `github-issues` backend. On completion, send `AppEvent::CacheRefresh { docs }` through the channel, triggering a `Store` merge and validation refresh.

The poll interval is configurable via `lazyspec.toml` under `[github-issues]`. Default: 60 seconds.

Files: `src/tui/infra/event_loop.rs`, `src/tui/state/app.rs`

## Test Plan

- `Store::load_with_fs` with a mock GitHub source returns `DocMeta` entries with `source: GithubIssue`
- Doc list snapshot test includes `[gh]` badge for GitHub-sourced documents
- Edit flow unit test: mock ETag match (no conflict), verify push is called
- Edit flow unit test: mock ETag mismatch, verify conflict overlay activates
- Status cycling unit test: `confirm_status_change` on a GithubIssue doc dispatches state change, not frontmatter rewrite
- Sync indicator renders correct color for fresh, aging, and stale timestamps
- Stale badge appears when cache age exceeds 2x TTL
- Background refresh: send `CacheRefresh` event, verify `last_sync` updates and store merges new docs
- Integration: `cargo test` passes with `--features agent` (no regressions in existing TUI tests)

## Notes

The `run_editor` suspend/restore pattern is well-established. The main risk is the optimistic lock check: if the user is offline when they close the editor, the push fails silently. Task 3 should queue a retry or surface the error in the status bar.

Rate limiting and HTTP client details are out of scope per STORY-098. The background refresh uses whatever transport the RFC-037 backend exposes.
