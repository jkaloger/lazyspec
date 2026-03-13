---
title: TUI warnings panel shows validation errors
type: iteration
status: accepted
author: agent
date: 2026-03-13
tags: []
related:
- implements: docs/stories/STORY-061-graceful-degradation-for-duplicate-ids.md
---



## Bug

The TUI warnings panel (`w` key) only displays `store.parse_errors()` -- documents that failed frontmatter parsing. It has no awareness of validation results from `validate_full()`, which is where duplicate-ID diagnostics, broken links, and rule violations are reported.

When two `RFC-021` documents exist, `cargo run -- validate --json` correctly reports `"duplicate id: RFC-021 (...)"` but the TUI warnings panel shows "No warnings".

**Root cause:** `draw_warnings_panel` in `src/tui/ui.rs:967` reads only `app.store.parse_errors()`. Validation results from `engine/validation.rs` are never computed or stored in the `App` state.

## Changes

### Task 1: Run validation and store results in App state

**ACs addressed:** STORY-061 AC-8 (TUI shows duplicates with warning indicator)

**Files:**
- Modify: `src/tui/app.rs`

**What to implement:**

Add a `validation_errors: Vec<String>` field to the `App` struct (around line 310). In `App::new()` and after `reload_store()`, run validation and store the error strings:

```
let result = crate::engine::validation::validate_full(&self.store, &self.config);
self.validation_errors = result.errors.iter().map(|e| e.to_string()).collect();
```

Also compute and store warnings in `validation_warnings: Vec<String>`.

**How to verify:** `cargo check` compiles. Add a unit test that builds an App with duplicate-ID docs and asserts `validation_errors` is non-empty.

### Task 2: Display validation errors in warnings panel

**ACs addressed:** STORY-061 AC-8

**Files:**
- Modify: `src/tui/ui.rs`

**What to implement:**

In `draw_warnings_panel` (line 967), after the parse errors section, also render `app.validation_errors`. Combine both sources into a single list. Parse errors get the existing yellow path + grey error format. Validation errors get a similar format but with the error string directly (they don't have a path+error structure, just a message string).

The `errors.is_empty()` check at line 993 should become `errors.is_empty() && app.validation_errors.is_empty()` so the "No warnings" message only shows when both are empty.

Update the content height calculation to account for validation errors too.

**How to verify:** Create two docs with the same ID prefix, open the TUI, press `w`. Both parse errors and validation errors (including duplicate ID) should appear.

## Test Plan

- `tui_warnings_shows_validation_errors`: Build an App with two `RFC-001-*.md` docs. Assert `app.validation_errors` contains a string matching "duplicate id". (Isolated, fast, behavioral)
- `tui_warnings_empty_when_no_issues`: Build an App with unique IDs. Assert `app.validation_errors` is empty. (Regression guard)

## Notes

The `"! "` prefix on duplicate IDs in the document list (`ui.rs:276`) appears correctly wired -- this iteration only addresses the warnings panel not showing validation results.
