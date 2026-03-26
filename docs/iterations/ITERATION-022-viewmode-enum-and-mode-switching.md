---
title: ViewMode Enum and Mode Switching
type: iteration
status: accepted
author: agent
date: 2026-03-06
tags: []
related:
- implements: STORY-012
---




## Scope

This iteration covers **AC1 through AC5** of STORY-012. It adds the `ViewMode` enum, backtick cycling, title bar mode indicator, rendering dispatch, and skeleton renderers for non-Types modes. Existing Types mode behaviour remains untouched.

A follow-up iteration can address any rendering polish, help text updates, or mode-specific content.

## Changes

### Task 1: Add ViewMode enum and state to App

**ACs addressed:** AC1

**Files:**
- Modify: `src/tui/app.rs`

**What to implement:**

Add a `ViewMode` enum with four variants: `Types`, `Filters`, `Metrics`, `Graph`. Derive `Debug, Clone, Copy, PartialEq`. Add a `next()` method that returns the next mode in the cycle (Types -> Filters -> Metrics -> Graph -> Types).

Add a `pub view_mode: ViewMode` field to the `App` struct, defaulting to `ViewMode::Types` in `App::new()`.

Add a `pub fn cycle_mode(&mut self)` method on `App` that sets `self.view_mode = self.view_mode.next()`.

**How to verify:**
```
cargo test
```
Unit tests in Task 5 will verify this.

---

### Task 2: Add backtick key handler

**ACs addressed:** AC2

**Files:**
- Modify: `src/tui/app.rs` (in `handle_normal_key`)

**What to implement:**

In `handle_normal_key`, add a match arm for `KeyCode::Char('`')` that calls `self.cycle_mode()`. Place it alongside the other single-key handlers (before the catch-all `_ => {}`).

**How to verify:**
```
cargo test
```
Unit tests in Task 5 will verify this.

---

### Task 3: Update title bar to show mode indicator

**ACs addressed:** AC3

**Files:**
- Modify: `src/tui/ui.rs` (in `draw`, around line 80-86)

**What to implement:**

Replace the current title bar rendering (which only shows "  lazyspec") with a line that includes the mode name and cycle hint. The format should match RFC-006:

```
  lazyspec                                    [Types] ` to cycle
```

Build the title `Line` with three spans:
1. `"  lazyspec"` in cyan/bold (existing)
2. A right-aligned gap (use `Span::raw` with padding, or render two `Paragraph` widgets in a horizontal layout)
3. `"[Types] ` to cycle"` in dark gray

A practical approach: use `Line::from(vec![...])` with the mode name span right-aligned. Since ratatui `Line` doesn't natively right-align spans, render the mode indicator as a separate right-aligned `Paragraph` in the same `outer[0]` area using `Alignment::Right`.

The mode name comes from a `Display` impl or a `fn name(&self) -> &str` method on `ViewMode`.

**How to verify:**
```
cargo run
```
Visual check that title bar shows mode and hint. Automated tests in Task 5 verify the state; rendering is visual.

---

### Task 4: Rendering dispatch and skeleton modes

**ACs addressed:** AC4, AC5

**Files:**
- Modify: `src/tui/ui.rs`

**What to implement:**

In the `draw` function, after rendering the title bar and splitting the outer layout, replace the current direct calls to `draw_type_panel`, `draw_doc_list`, `draw_preview` with a match on `app.view_mode`:

```rust
match app.view_mode {
    ViewMode::Types => {
        // existing layout: left type panel + right doc list/preview
        // move the current main/right layout split and draw calls here
    }
    ViewMode::Filters => draw_filters_skeleton(f, outer[1]),
    ViewMode::Metrics => draw_metrics_skeleton(f, outer[1]),
    ViewMode::Graph => draw_graph_skeleton(f, outer[1]),
}
```

The delete confirm and help overlays remain outside the match (they render on top regardless of mode).

Add three new functions:

- `fn draw_filters_skeleton(f: &mut Frame, area: Rect)` -- renders a two-panel layout (left "Filters" block, right "Documents" block) with borders and titles only, no content.
- `fn draw_metrics_skeleton(f: &mut Frame, area: Rect)` -- renders a two-panel layout (left "Metrics" block, right "Status Flow" block) with borders and titles only.
- `fn draw_graph_skeleton(f: &mut Frame, area: Rect)` -- renders a two-panel layout (left "Graph" block, right "Dependency Graph" block) with borders and titles only.

Each skeleton uses the same 20%/80% horizontal split as the Types mode for visual consistency.

**How to verify:**
```
cargo run
```
Press backtick to cycle through modes. Each non-Types mode shows titled, bordered skeleton panels. Types mode renders identically to before.

---

### Task 5: Tests

**ACs addressed:** AC1, AC2, AC5

**Files:**
- Create: `tests/tui_view_mode_test.rs`

**What to implement:**

Following the patterns in `tests/tui_handle_key_test.rs`:

1. **`test_app_defaults_to_types_mode`** -- Create an `App`, assert `app.view_mode == ViewMode::Types`. (AC1)

2. **`test_view_mode_next_cycles`** -- Assert the full cycle: Types -> Filters -> Metrics -> Graph -> Types. (AC2 logic)

3. **`test_backtick_cycles_mode`** -- Create an `App`, send `KeyCode::Char('`')`, assert `app.view_mode == ViewMode::Filters`. Send again, assert `Metrics`. (AC2)

4. **`test_types_mode_navigation_unchanged`** -- Create an `App` with docs, verify `j/k/h/l/Enter/Tab` all work the same as before (selected_doc changes, type changes, fullscreen toggles, preview tab toggles). This replicates a subset of existing handle_key tests but explicitly asserts `view_mode` stays `Types`. (AC5)

5. **`test_backtick_ignored_in_modal_states`** -- Enter search mode, send backtick, assert mode is still `Types`. Same for fullscreen, create form, delete confirm. (AC5 safety)

**How to verify:**
```
cargo test tui_view_mode
```

## Test Plan

| Test | ACs | Properties |
|------|-----|-----------|
| `test_app_defaults_to_types_mode` | AC1 | Isolated, Fast, Specific |
| `test_view_mode_next_cycles` | AC2 | Isolated, Fast, Deterministic |
| `test_backtick_cycles_mode` | AC2 | Isolated, Behavioral, Specific |
| `test_types_mode_navigation_unchanged` | AC5 | Behavioral, Structure-insensitive |
| `test_backtick_ignored_in_modal_states` | AC5 | Behavioral, Predictive |

All tests are unit-level against `App` state. No rendering tests -- the skeleton panels are verified visually during build. This keeps tests fast and deterministic.

## Notes

- The backtick key is `KeyCode::Char('\u{0060}')` in crossterm. Verify the exact representation during build.
- Skeleton renderers intentionally have no interactivity. Future iterations for each mode (Filters, Metrics, Graph) will replace the skeletons with real content.
- The `ViewMode::next()` method is a pure function on the enum, keeping cycling logic testable without the full `App`.
