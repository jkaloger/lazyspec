---
title: Warnings panel fix action and q dismiss
type: iteration
status: accepted
author: agent
date: 2026-03-08
tags: []
related:
- implements: docs/stories/STORY-046-tui-warnings-panel.md
---



## Changes

### Task 1: Add `q` dismiss and `f` fix action

**Files:**
- Modify: `src/tui/app.rs`
- Modify: `src/tui/mod.rs`

**What to implement:**

In `app.rs`:

Add a field `pub fix_request: bool` to `App` (default `false`). Initialize in `App::new()`.

In `handle_warnings_key`, add two match arms:
- `KeyCode::Char('q')` => `self.close_warnings()` (dismiss, same as Esc/w)
- `KeyCode::Char('f')` => set `self.fix_request = true` and `self.close_warnings()`

In `mod.rs`, in the main TUI loop (after the `editor_request` block, before `should_quit`):

```rust
if app.fix_request {
    app.fix_request = false;
    let root = app.store.root().to_path_buf();
    let paths: Vec<String> = app.store.parse_errors()
        .iter()
        .map(|e| e.path.to_string_lossy().to_string())
        .collect();
    crate::cli::fix::run(&root, &app.store, config, &paths, false, false);
    app.store = Store::load(&root, config);
}
```

This runs fix on all errored files, then reloads the store so the warnings list reflects the new state.

**How to verify:**
`cargo test` -- verified by Task 2 tests.


### Task 2: Tests

**Files:**
- Modify: `tests/tui_warnings_test.rs`

**What to implement:**

Add tests:

1. `test_handle_key_q_closes_warnings` -- open warnings, simulate `KeyCode::Char('q')` via `handle_key()`, assert `show_warnings == false`.
2. `test_handle_key_f_sets_fix_request` -- open warnings (with parse errors), simulate `KeyCode::Char('f')` via `handle_key()`, assert `fix_request == true` and `show_warnings == false`.

**How to verify:**
`cargo test tui_warnings`


## Test Plan

| Test | What it verifies |
|------|-----------------|
| q closes warnings | `q` dismisses the panel |
| f sets fix_request | `f` triggers fix and closes panel |

Both are behavioral state-transition tests, consistent with the existing warnings test suite.

## Notes

The fix action follows the same deferred-action pattern as `editor_request` -- the key handler sets a flag, and the main loop processes it. This avoids running CLI commands inside the key handler.
