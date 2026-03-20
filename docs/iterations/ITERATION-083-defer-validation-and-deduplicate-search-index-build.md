---
title: Defer validation and deduplicate search index build
type: iteration
status: draft
author: jkaloger
date: 2026-03-19
tags: []
related:
- implements: docs/stories/STORY-069-reduce-time-to-first-frame-on-tui-startup.md
---


## Task Breakdown

### Task 1: Remove duplicate `rebuild_search_index()` call (F5)

**ACs addressed:** AC2

**Files:**
- M `src/tui/app.rs` (line 428)

Remove `app.rebuild_search_index()` from `App::new()`. The call at `app.rs:438` inside `refresh_validation()` will handle it. This is a one-line delete.

Verify that every call site of `App::new()` is followed by `refresh_validation()`. Currently there's only one call site (`mod.rs:109-110`), plus `Store::load` + `refresh_validation` pairs after editor/fix operations (`mod.rs:225, 245-246, 257-258`). Those paths construct a fresh `App` via `Store::load` but reuse the existing `app`, so they call `refresh_validation` directly and are unaffected.

### Task 2: Defer validation to after first frame (F3)

**ACs addressed:** AC1, AC3

**Files:**
- M `src/tui/mod.rs` (line 110)
- M `src/tui/app.rs` (around line 433)

Remove the `app.refresh_validation(config)` call at `mod.rs:110`. Instead, send an `AppEvent::ValidationRequest` into the channel immediately after channel creation so it's the first event processed by the main loop. Add a handler in `handle_app_event` that calls `app.refresh_validation(config)`.

This means the first frame renders with empty validation state (no errors/warnings shown). The validation results appear on the second frame, ~16ms later. The UI already handles empty validation state since `validation_errors` and `validation_warnings` are initialized as empty `Vec`s in `App::new()`.

An alternative is spawning validation in a background thread, but that would require making `Store` + `Config` thread-safe or cloning them. The single-event approach is simpler and achieves the goal of rendering before validating.

## Test Plan

- Launch the TUI on a codebase with known validation errors. Confirm validation indicators appear within the first second (they should appear on the second frame).
- Launch the TUI on a clean codebase. Confirm no flash of false validation state.
- `LAZYSPEC_PERF_LOG=1`: compare first-frame timing before and after. The `draw` duration for loop #1 should be measurably lower since it no longer waits for validation.
- Run `cargo test` to verify no test depends on validation being populated at construction time.

## Notes

Task 1 should be done first since it's trivial and reduces the search index overhead that Task 2's deferred validation will trigger. The two tasks are otherwise independent.
