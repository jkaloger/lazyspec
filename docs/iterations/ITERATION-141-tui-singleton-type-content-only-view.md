---
title: TUI singleton type content-only view
type: iteration
status: accepted
author: agent
date: 2026-03-29
tags: []
related:
- implements: STORY-103
---




## Summary

When a singleton document type (e.g. convention) is selected in the TUI, the doc list table is redundant — there's at most one document. Skip the list table entirely and give the full right-side area to the content preview.

## Changes

### Task 1: Skip doc list for singleton types in the TUI layout

**ACs addressed:** Extends singleton-config-deserialization (the TUI now consumes the singleton flag)

**Files:**
- Modify: `src/tui/views.rs` (lines 135-148, the `ViewMode::Types` layout branch)

**What to implement:**

In `draw()`, the `ViewMode::Types` branch currently always renders a 3-panel layout:
```
type_panel (20%) | doc_list (40% height)
                 | preview  (60% height)
```

Check whether the current type is a singleton via `config.type_by_name(app.current_type().as_str())`. If the type is a singleton:
- Skip `draw_doc_list()` entirely
- Give the full `main[1]` area to `draw_preview()` (no vertical split needed)

If not a singleton, keep the existing 3-panel layout unchanged.

The check is straightforward:
```rust
let is_singleton = config
    .type_by_name(app.current_type().as_str())
    .map(|td| td.singleton)
    .unwrap_or(false);
```

Then branch on `is_singleton` to decide the layout.

**How to verify:**
- `cargo run -- tui` → select the convention type → doc list table should not appear, content fills the right panel
- Select a non-singleton type (e.g. rfc) → layout unchanged

### Task 2: Auto-select the singleton document

**ACs addressed:** Extends singleton-config-deserialization

**Files:**
- Modify: `src/tui/state/app.rs` (in `build_doc_tree()` around line 380, and/or `move_type_next()`/`move_type_prev()`)

**What to implement:**

When the user navigates to a singleton type, `build_doc_tree()` runs and populates `doc_tree` with 0 or 1 entries. `selected_doc` is already reset to 0 by `move_type_next()`/`move_type_prev()`, so the single document is automatically selected. No change needed to selection logic.

However, verify that the preview panel correctly renders content when `doc_tree` has exactly one item and `selected_doc == 0`. Read `draw_preview()` to confirm it reads `app.doc_tree[app.selected_doc]` and handles the case. If the preview relies on the body cache (`expanded_body_cache`), ensure it triggers expansion for the auto-selected document.

This task may be a no-op if auto-selection already works. Verify before writing code.

**How to verify:**
- `cargo run -- tui` → navigate to convention type → preview should show the convention document content without needing to press any key to select it

## Test Plan

### Test 1: Singleton type skips doc list (behavioral, structure-insensitive)

Set up an `App` with a config containing a singleton type and one document of that type. Call the draw function (or the layout logic) and verify that `draw_doc_list` is not invoked / the layout gives full area to the preview.

Since the draw function writes to a `Frame` (ratatui terminal), the most practical test is a TUI integration test that:
1. Creates a temp project with a singleton type configured
2. Creates one document of that type
3. Initializes `App`, sets `selected_type` to the singleton type
4. Renders a frame to a `TestBackend`
5. Asserts the doc list table is not present in the rendered buffer (no table header row for the list)

**Tradeoff:** This is an integration test (sacrifices Fast for Predictive). The layout branch is a 5-line conditional — a unit test would be coupled to implementation details. The integration test verifies the user-visible behavior.

### Test 2: Non-singleton type still shows doc list (regression guard)

Same setup as Test 1 but with a non-singleton type. Assert the doc list table IS present.

## Notes

The `TypeDef.singleton` field is already deserialized and available on `Config`. The TUI's `draw()` already receives `&Config`. This is a pure presentation change with no engine modifications.
