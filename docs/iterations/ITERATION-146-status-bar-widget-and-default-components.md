---
title: Status bar widget and default components
type: iteration
status: accepted
author: agent
date: 2026-03-30
tags: []
related:
- implements: STORY-105
---



## Changes

### Task 1: Create status bar module with rendering logic

**ACs addressed:** AC-1 (visible in bottom row with distinct background), AC-10 (separator between components), AC-11 (empty components omitted)

**Files:**
- Create: `src/tui/views/status_bar.rs`
- Modify: `src/tui/views.rs` (add `mod status_bar;`)

**What to implement:**

A `StatusBar` struct that holds three zones (left, center, right), each containing an ordered list of `Option<Span>` values. The rendering method:

1. Filters out `None` entries per zone
2. Joins remaining spans with ` │ ` separators
3. Composes a single `Line` with left-aligned, centered, and right-aligned sections
4. Renders as a `Paragraph` widget with `Color::DarkGray` background and `Color::White` default foreground

Expose a `draw_status_bar(f: &mut Frame, app: &App, area: Rect)` function. The function evaluates all registered component functions against `&App`, collects spans per zone, and renders.

Use ANSI-compatible colors only (the base `Color` variants in ratatui, not RGB).

**How to verify:**
- `cargo test` passes
- Unit test: construct a StatusBar with known spans, render to a test backend, assert the output contains expected text and separators
- Unit test: include a `None` component, assert no extra separator appears

---

### Task 2: Implement built-in components

**ACs addressed:** AC-2 (mode name), AC-3 (doc count), AC-4 (warnings in yellow), AC-5 (errors in red), AC-6 (omit zero counts), AC-7 (version), AC-8 (help hint)

**Files:**
- Modify: `src/tui/views/status_bar.rs`

**What to implement:**

Each component is a function `fn(&App) -> Option<Span>`:

- `mode_component`: returns `app.view_mode.name()` as a span. Style with bold text and a mode-specific background color (like lualine's mode coloring).
- `doc_count_component`: returns `format!("{} docs", app.doc_tree.len())`. Returns `None` if count is zero.
- `warnings_component`: returns `format!("{} ⚠", count)` in `Color::Yellow` where count = `app.validation_warnings.len()`. Returns `None` if zero (AC-6).
- `errors_component`: returns `format!("{} ✗", count)` in `Color::Red` where count = `app.validation_errors.len() + app.store.parse_errors().len()`. Returns `None` if zero (AC-6).
- `version_component`: returns `format!("lazyspec v{}", env!("CARGO_PKG_VERSION"))`. Always `Some`.
- `help_hint_component`: returns `"? help"` in `Color::DarkGray` foreground. Always `Some`.

Wire these into `draw_status_bar` with default zone assignments:
- Left: `[mode, doc_count]`
- Center: `[warnings, errors]`
- Right: `[version, help_hint]`

**How to verify:**
- `cargo test` passes
- Unit test per component: given known App state, assert correct `Some`/`None` and span content
- Unit test: warnings/errors return `None` when counts are zero

---

### Task 3: Integrate status bar into draw()

**ACs addressed:** AC-1 (bottom row), AC-9 (standalone help hint removed/absorbed)

**Files:**
- Modify: `src/tui/views.rs` (lines 91-94, and early-return paths at lines 60-89)

**What to implement:**

In the main `draw()` function:

1. Change the outer layout from `[Length(1), Min(0)]` to `[Length(1), Min(0), Length(1)]` -- title bar, content, status bar.
2. After overlay rendering (line ~203), call `draw_status_bar(f, app, outer[2])`.
3. For the early-return paths (fullscreen at line 60, create_form at line 70, search at line 80): do NOT render the status bar in fullscreen mode (per RFC-022 design). Do render it in create_form and search modes by calling `draw_status_bar` before the early return.

For the fullscreen early-return path: the status bar is deliberately hidden to maximize preview space (RFC-022 spec). The `[Length(1), Min(0), Length(1)]` layout should only apply in the non-fullscreen path. In fullscreen, keep the current `f.area()` usage.

The standalone `? help` hint from RFC-011 was never implemented as a persistent label, so there is no code to remove (AC-9 is satisfied by the `help_hint` component existing in the status bar).

**How to verify:**
- `cargo test` passes
- `cargo run` launches TUI with visible status bar in bottom row
- Switching modes updates the mode component
- Entering fullscreen hides the status bar
- Help hint visible in status bar right section

## Test Plan

**Unit tests** (in `src/tui/views/status_bar.rs`):

- `test_status_bar_renders_all_zones`: construct StatusBar with spans in all three zones, render to ratatui `TestBackend`, assert left/center/right content appears in correct positions. Isolated, fast, deterministic.
- `test_empty_component_omitted`: include a `None` in a zone's component list, assert no double-separator or blank space. Tests AC-11.
- `test_separator_between_components`: two `Some` spans in same zone produce ` │ ` between them. Tests AC-10.
- `test_mode_component_returns_mode_name`: set `app.view_mode` to each variant, assert output matches `view_mode.name()`. Tests AC-2.
- `test_doc_count_component`: set `app.doc_tree` to known length, assert output contains that count. Tests AC-3.
- `test_warnings_component_yellow`: set `app.validation_warnings` to non-empty, assert span is `Color::Yellow`. Returns `None` when empty. Tests AC-4, AC-6.
- `test_errors_component_red`: set `app.validation_errors` + `app.store.parse_errors()` to non-empty, assert span is `Color::Red`. Returns `None` when empty. Tests AC-5, AC-6.
- `test_version_component`: assert output contains `env!("CARGO_PKG_VERSION")`. Tests AC-7.
- `test_help_hint_component`: assert returns `Some` with `"? help"`. Tests AC-8.

Tradeoff: these are unit tests against ratatui's `TestBackend`, not full integration tests. They sacrifice Predictive (can't verify actual terminal rendering) for Fast and Isolated. The manual verification in Task 3 covers the integration gap.

## Notes

- The `? help` persistent hint from RFC-011 was never implemented as a standalone label. The status bar's `help_hint` component is the first time it appears persistently. No existing code needs to be removed.
- `app.store.parse_errors()` is separate from `validation_errors`. The errors component should count both to give an accurate picture.
- Fullscreen mode hides the status bar per RFC-022 design. This is Story 2 scope technically (listed under "Interaction with Existing UI"), but since the fullscreen early-return path must be handled during integration anyway, it's addressed here.
