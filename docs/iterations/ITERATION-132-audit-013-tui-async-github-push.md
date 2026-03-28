---
title: AUDIT-013 TUI async GitHub push
type: iteration
status: accepted
author: agent
date: 2026-03-28
tags: []
related:
- related-to: AUDIT-013
---



## Changes

### Task 1: Move `try_push_gh_edit` to a background thread

Audit finding 15. `try_push_gh_edit` at `src/tui/infra/event_loop.rs:71-109` runs synchronously after the editor closes (called at line 388). It creates a `GhCli`, loads `IssueMap`, and makes API calls. On slow connections this freezes the TUI.

Move the push to a background thread, mirroring the existing `CacheRefresh` pattern (lines 332-370):

1. Add an `AppEvent::GhPushResult(Result<(), String>)` variant
2. After the editor closes, spawn a thread that runs `try_push_gh_edit` and sends the result via `AppEvent::GhPushResult`
3. Add a "pushing..." indicator to the TUI status bar while the push is in flight (use an `AtomicBool` flag like `refresh_in_flight`)
4. In `handle_app_event`, process `GhPushResult` to clear the indicator or show the conflict message

The existing `gh_conflict_message` field on `App` (line 389) already handles error display.

AC: Editor close returns to TUI immediately. Push happens in background. Conflict messages still display. `cargo test` passes.

### Task 2: Share `GithubIssuesStore` instance within the TUI event loop

Audit finding 16. Every call to `try_push_gh_edit` constructs a fresh `GithubIssuesStore` (lines 95-104), loading `IssueMap` from disk each time. The polling thread (lines 340-366) maintains its own `IssueMap`. Two independent copies can diverge.

After ITERATION-131 removes `RefCell`, the store uses `&mut self`. For cross-thread sharing, wrap in `Arc<Mutex<GithubIssuesStore>>`:

1. Construct the `GithubIssuesStore` once at TUI startup (near existing config/store setup)
2. Wrap in `Arc<Mutex<_>>`, share between push and polling threads
3. Polling thread updates the shared `IssueMap` after fetching (currently loads its own at line 340)
4. Push thread reads the shared `IssueMap` for optimistic lock checks

Remove per-call store construction in `try_push_gh_edit`. Use `Option<Arc<Mutex<...>>>` to handle the case where no github-issues types are configured.

AC: Single `GithubIssuesStore` instance shared across push and poll. No per-call `IssueMap::load`. `cargo test` passes.

## Test Plan

- `cargo test` passes after each task
- Manual: open TUI, edit a github-issues document, verify editor returns immediately and push completes in background
- Manual: verify conflict message still appears if the issue was modified externally
- Manual: verify polling and push don't deadlock (edit during a poll cycle)

## Notes

Depends on ITERATION-131 because removing `RefCell` (finding 7) changes the ownership model. With `&mut self` on `DocumentStore`, `Arc<Mutex<_>>` is the natural cross-thread sharing pattern.
