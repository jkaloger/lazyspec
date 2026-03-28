---
title: GitHub Issues TUI Integration
type: iteration
status: accepted
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

## Prerequisite

`Store::load_with_fs` already redirects types with `store = "github-issues"` to `.lazyspec/cache/<type_name>/`, so GH-backed documents load into the `Store` as regular `DocMeta` entries without further changes. The document model does not need a `source` field; provenance is determined at runtime by looking up the document's `TypeDef.store` from `Config`.

GitHub API operations use the inline-construction pattern established in `cli/update.rs`: build a fresh `GithubIssuesStore<GhCli>` per operation using `Config` (which flows through the event loop as `&Config`). The TUI `App` struct does not need to hold a `GhClient` or `GithubIssuesStore`.

## Task Breakdown

### Task 1: `[gh]` badge in document list

The panels code (`src/tui/views/panels.rs`) iterates `Store::all_docs()` via `doc_row_cells`. To render a `[gh]` badge in the ID column for GH-backed documents, pass `Config` into the rendering path so `doc_row_cells` can resolve `TypeDef.store` for each document's type. The badge is appended to the ID cell when `store == StoreBackend::GithubIssues`.

Files: `src/tui/views/panels.rs`, `src/tui/views.rs` (thread `&Config` through `draw`)

### Task 2: Edit flow with optimistic locking

When the user presses `e` on a GitHub Issues document:

1. Resolve the doc's `TypeDef` from `Config`. If `store == GithubIssues`, enter the GH edit path.
2. Construct a `GithubIssuesStore<GhCli>` inline (same pattern as `cli/update.rs`).
3. Load `IssueMap`, look up the `updated_at` ETag for the document.
4. Write cached content to a temp file, launch `$EDITOR` via the existing `editor_request` / `run_editor` / `input_paused` pattern in `event_loop.rs`.
5. On return, parse the temp file. Compare the stored ETag against the remote `updated_at`.
6. If they match, push the update via `GithubIssuesStore::update`.
7. If they differ, show a conflict overlay warning the user before proceeding.

The branching point is the `editor_request.take()` block in `event_loop.rs`. After `run_editor` returns, check whether the edited path belongs to a GH-backed type and run the post-push logic before `reload_file`.

Files: `src/tui/infra/event_loop.rs`, `src/tui/state/app.rs`, `src/tui/views/overlays.rs` (conflict dialog)

### Task 3: Status cycling

`confirm_status_change` in `src/tui/state/app.rs` already receives `&Config` and delegates to `cli::update::run_with_config`, which constructs a `GithubIssuesStore<GhCli>` inline when the type's store is `GithubIssues`. This path already works for GH-backed documents.

The remaining work is in the `StatusPicker` UI. For GH-backed documents, the picker should present only the statuses that map to GitHub state transitions (open/closed) per RFC-037's status mapping table. The branching point is `handle_status_picker_key` in `src/tui/views/keys.rs`: resolve the document's `TypeDef.store` from `Config` and filter the available statuses accordingly.

Files: `src/tui/views/keys.rs`, `src/tui/views/overlays.rs`

### Task 4: Sync indicator in title bar

Add a `last_sync: Option<Instant>` field to `App`. The background cache refresh (Task 6) updates this on each successful poll. The title bar in `src/tui/views.rs` renders a right-aligned indicator in `outer[0]`, between the title and the existing mode-indicator hint:

- `synced 12s ago` in green when fresh (within `cache_ttl`)
- `synced 2m ago` in yellow when approaching 2x TTL
- `stale` in red when beyond 2x TTL

The TTL value comes from `Config.documents.github.cache_ttl`. Only render the indicator when at least one type uses `store = "github-issues"`.

Files: `src/tui/views.rs`, `src/tui/state/app.rs`

### Task 5: Stale document warning badge

Documents whose cache age exceeds 2x TTL get a `[!]` badge in the status column of the doc list. The TTL threshold comes from `Config.documents.github.cache_ttl` (the `[github]` config section). Cache timestamps come from `IssueMap` entries (`updated_at`) or file mtime of the cached markdown in `.lazyspec/cache/<type>/`.

Files: `src/tui/views/panels.rs`

### Task 6: Background cache refresh

Add a new `AppEvent::CacheRefresh` variant. In the event loop, track a `next_poll: Instant`. When the loop iteration fires past `next_poll`, spawn a background thread that constructs a `GithubIssuesStore<GhCli>` inline, calls the cache refresh function, and sends `AppEvent::CacheRefresh { docs }` through the channel. On receipt, merge into `Store`, update `last_sync`, and refresh validation.

The poll interval comes from `Config.documents.github.cache_ttl` in the `[github]` section. Default: 60 seconds.

Files: `src/tui/infra/event_loop.rs`, `src/tui/state/app.rs`

## Test Plan

- Doc list snapshot test includes `[gh]` badge for documents whose `TypeDef.store` is `GithubIssues`
- Edit flow unit test: mock `GhClient` with matching ETag, verify `update` is called
- Edit flow unit test: mock `GhClient` with mismatched ETag, verify conflict overlay activates
- Status cycling: `confirm_status_change` on a GH-backed doc dispatches through `run_with_config` (existing path, verify no regression)
- Status picker renders filtered status list for GH-backed documents
- Sync indicator renders correct color for fresh, aging, and stale timestamps
- Stale badge appears when cache age exceeds 2x TTL
- Background refresh: send `CacheRefresh` event, verify `last_sync` updates and store merges new docs
- Integration: `cargo test` passes with `--features agent` (no regressions)

## Notes

The `run_editor` suspend/restore pattern is well-established. The main risk is the optimistic lock check: if the user is offline when they close the editor, the push fails. Task 2 should surface the error in the title bar sync indicator rather than failing silently.

Rate limiting and HTTP client details are out of scope per STORY-098. The background refresh uses whatever transport the `GhClient` trait exposes (currently `GhCli` shelling out to `gh`).
