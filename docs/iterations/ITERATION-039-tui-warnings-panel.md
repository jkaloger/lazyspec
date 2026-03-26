---
title: TUI warnings panel
type: iteration
status: accepted
author: agent
date: 2026-03-08
tags: []
related:
- implements: STORY-046
---




## Changes

### Task 1: Add warnings state to App

**ACs addressed:** AC1, AC3, AC4, AC5

**Files:**
- Modify: `src/tui/app.rs`

**What to implement:**

Add two fields to `App` struct:
- `pub show_warnings: bool` (default `false`)
- `pub warnings_selected: usize` (default `0`)

Add methods:
- `open_warnings(&mut self)` -- only sets `show_warnings = true` and resets `warnings_selected = 0` if `self.store.parse_errors()` is non-empty. No-op otherwise (AC5).
- `close_warnings(&mut self)` -- sets `show_warnings = false` and resets `warnings_selected = 0`.
- `warnings_move_up(&mut self)` -- decrements `warnings_selected` with floor of 0 (same pattern as `search_move_up`).
- `warnings_move_down(&mut self)` -- increments `warnings_selected` clamped to `parse_errors.len() - 1` (same pattern as `search_move_down`).

Add `handle_warnings_key(&mut self, code: KeyCode)`:
- `KeyCode::Esc | KeyCode::Char('w')` => `self.close_warnings()`
- `KeyCode::Char('j') | KeyCode::Down` => `self.warnings_move_down()`
- `KeyCode::Char('k') | KeyCode::Up` => `self.warnings_move_up()`
- `_` => no-op

Wire into `handle_key()`: add `if self.show_warnings { return self.handle_warnings_key(code); }` after the `show_help` check and before `create_form.active` check.

Wire `'w'` in `handle_normal_key()`: add `KeyCode::Char('w') => self.open_warnings()` in both the `ViewMode::Filters` and default match arms, alongside existing keybindings like `'?'` and `'/'`.

**How to verify:**
`cargo test` -- verified by Task 3 tests.


### Task 2: Render the warnings panel

**ACs addressed:** AC1, AC2, AC3

**Files:**
- Modify: `src/tui/ui.rs`

**What to implement:**

Add `draw_warnings_panel(f: &mut Frame, app: &App)` function. Follow the `draw_delete_confirm` centered popup pattern:

1. Calculate popup size: width = `min(70, area.width - 4)`, height = `min(parse_errors.len() + 4, area.height - 4)` (title bar + borders + at least one line of content).
2. Center the popup using the same x/y calculation as delete confirm.
3. `f.render_widget(Clear, popup_area)` to clear the background.
4. Build `Vec<ListItem>` from `app.store.parse_errors()`. Each item shows the path on one line styled with `Color::Yellow`, and the error message on the next line styled with `Color::DarkGray`. Use the two-line `ListItem` pattern.
5. Render as a `List` widget with `ListState` seeded from `app.warnings_selected`. Use `highlight_style` with `Color::Cyan` and `Modifier::BOLD`.
6. Wrap in a `Block` with `Borders::ALL`, `BorderType::Rounded`, `border_style` `Color::Yellow`, title `" Warnings (w/Esc to close) "`.

In `draw()`, add the warnings panel rendering after `delete_confirm` and before `show_help` in the normal view path (lines 118-124). This makes it an overlay on the main view:

```rust
if app.show_warnings {
    draw_warnings_panel(f, app);
}
```

Also add early return path for fullscreen/create_form/search_mode blocks (same pattern as help overlay -- render warnings panel if `app.show_warnings` is true within those early-return blocks).

**How to verify:**
`cargo test` -- rendering is implicitly verified by state tests. Manual visual check for layout.


### Task 3: Tests

**ACs addressed:** AC1, AC2, AC3, AC4, AC5

**Files:**
- Create: `tests/tui_warnings_test.rs`

**What to implement:**

Use the same `TestFixture` pattern as `tui_search_test.rs` and `tui_delete_dialog_test.rs`.

Helper: `setup_app_with_parse_errors()` -- creates a fixture with one valid doc and one or more broken docs (missing required frontmatter fields so they produce parse errors). Returns `(TestFixture, App)`.

Helper: `setup_app_no_errors()` -- creates a fixture with only valid docs. Returns `(TestFixture, App)`.

Tests:

1. `test_open_warnings_with_errors` -- call `open_warnings()`, assert `show_warnings == true` and `warnings_selected == 0`. (AC1)
2. `test_open_warnings_no_errors_is_noop` -- use no-errors fixture, call `open_warnings()`, assert `show_warnings == false`. (AC5)
3. `test_close_warnings` -- open then close, assert `show_warnings == false` and `warnings_selected == 0`. (AC4)
4. `test_warnings_move_down` -- open warnings, call `warnings_move_down()`, assert `warnings_selected == 1`. (AC3)
5. `test_warnings_move_down_clamps` -- open warnings with N errors, call `warnings_move_down()` N+5 times, assert `warnings_selected == N-1`. (AC3)
6. `test_warnings_move_up` -- open, move down twice, move up once, assert `warnings_selected == 1`. (AC3)
7. `test_warnings_move_up_clamps_at_zero` -- open, call `warnings_move_up()`, assert `warnings_selected == 0`. (AC3)
8. `test_handle_key_w_toggles_warnings` -- simulate `KeyCode::Char('w')` in normal mode, assert `show_warnings == true`. Simulate again, assert `show_warnings == false`. (AC1, AC4)
9. `test_handle_key_esc_closes_warnings` -- open warnings, simulate `KeyCode::Esc`, assert `show_warnings == false`. (AC4)
10. `test_warnings_intercepts_keys` -- open warnings, simulate `KeyCode::Char('q')`, assert `should_quit == false` (panel absorbs the key). (AC1)

**How to verify:**
`cargo test tui_warnings`


## Test Plan

All tests use `TestFixture` with broken frontmatter docs to generate `ParseError` entries in the Store. Tests are behavioral (testing state transitions, not rendering internals) and deterministic.

| Test | ACs | Properties traded |
|------|-----|-------------------|
| open with errors | AC1 | - |
| open no errors is noop | AC5 | - |
| close resets state | AC4 | - |
| move down increments | AC3 | - |
| move down clamps | AC3 | - |
| move up decrements | AC3 | - |
| move up clamps at zero | AC3 | - |
| w toggles on/off | AC1, AC4 | - |
| esc closes | AC4 | - |
| key interception | AC1 | - |

AC2 (path + error display) is verified structurally through the render function using `parse_errors()` data. The render function is straightforward list construction, so a rendering test would be structure-sensitive without adding confidence. AC2 is better verified through manual visual check.

## Notes

The warnings panel follows the established overlay pattern (delete confirm, help). It uses a centered popup rather than a full-screen takeover because the error list is typically short.
