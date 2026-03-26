---
title: Scroll padding and half-page navigation
type: iteration
status: accepted
author: agent
date: 2026-03-08
tags: []
related:
- implements: STORY-048
---




## Changes

### Task 1: Add `doc_list_offset` field and manual viewport management

**ACs addressed:** AC1, AC2, AC3, AC4

**Files:**
- Modify: `src/tui/app.rs` (App struct, `move_down`, `move_up`, `move_to_top`, `move_to_bottom`)
- Modify: `src/tui/ui.rs` (`draw_doc_list`)

**What to implement:**

Add a `doc_list_offset: usize` field to `App`. This tracks the first visible row in the document list, replacing `TableState`'s auto-scroll.

Introduce a constant `SCROLL_PADDING: usize = 2`.

Introduce a method `adjust_viewport(&mut self, visible_height: usize)` that enforces scrolloff logic:
- If `selected_doc < doc_list_offset + SCROLL_PADDING`, set `doc_list_offset = selected_doc.saturating_sub(SCROLL_PADDING)`.
- If `selected_doc >= doc_list_offset + visible_height - SCROLL_PADDING`, set `doc_list_offset = selected_doc + SCROLL_PADDING + 1 - visible_height` (clamped to `max(0, doc_count - visible_height)`).
- Clamp `doc_list_offset` so it never exceeds `doc_count.saturating_sub(visible_height)`.

The sticky viewport for scroll-up (AC3) is inherent: the viewport only adjusts when the selection reaches the padding boundary, not on every keystroke.

In `draw_doc_list`, instead of creating `TableState::default().with_selected(Some(app.selected_doc))`, create the state with both `selected` and `offset`:
```rust
let mut state = TableState::default()
    .with_selected(Some(app.selected_doc))
    .with_offset(app.doc_list_offset);
```

The `visible_height` calculation: `area.height.saturating_sub(2)` (subtract 2 for the block border). Pass this back to the app so `adjust_viewport` can run. One approach: call `app.adjust_viewport(visible_height)` at the start of `draw_doc_list` (requires `&mut App`), or store `last_visible_height` on `App` and call `adjust_viewport` in the movement methods.

Recommended approach: store `doc_list_height: usize` on `App`, set it during `draw_doc_list` (change `app: &App` to `app: &mut App`), and call `adjust_viewport` inside `move_down`/`move_up`/`move_to_top`/`move_to_bottom` using that stored height.

Update `move_to_top` and `move_to_bottom` to also reset `doc_list_offset` (0 for top, clamped max for bottom).

Also apply the same logic to Filters mode navigation (lines 1013-1022 in `app.rs`), which inlines its own j/k handling. Extract the filtered doc count and apply the same viewport adjustment.

**How to verify:**
```
cargo test
cargo run
# Navigate a long document list with j/k, confirm 2-row padding at edges
# Scroll down, then scroll up -- viewport should hold until selection hits top padding
# Navigate near first/last item -- no blank rows or panics
```

### Task 2: Half-page jumps in document lists (`Ctrl-D` / `Ctrl-U`)

**ACs addressed:** AC5, AC6, AC7

**Files:**
- Modify: `src/tui/app.rs` (`handle_normal_key` default mode match, Filters mode match)

**What to implement:**

Add two methods to `App`:
- `half_page_down(&mut self)`: move `selected_doc` by `doc_list_height / 2`, clamped to the last item. Call `adjust_viewport` after.
- `half_page_up(&mut self)`: move `selected_doc` backward by `doc_list_height / 2`, clamped to 0. Call `adjust_viewport` after.

In the **default mode** match block (line 1115), add arms before the catch-all:
```rust
(KeyCode::Char('d'), KeyModifiers::CONTROL) => self.half_page_down(),
(KeyCode::Char('u'), KeyModifiers::CONTROL) => self.half_page_up(),
```

In the **Filters mode** match block (line 997), the match is on raw `KeyCode` without modifiers. Either:
- Refactor to match on `(code, modifiers)` like the default mode, or
- Add a guard: check `modifiers.contains(KeyModifiers::CONTROL)` before the existing `j`/`k` arms.

Recommended: refactor the Filters match to use `(code, modifiers)` tuples for consistency. Add `Ctrl-D`/`Ctrl-U` arms that call `half_page_down`/`half_page_up` with the filtered doc count as the boundary.

For Filters mode, `half_page_down`/`half_page_up` need to know the list length. The filtered doc count differs from `doc_tree.len()`. Use a parameter or have the methods accept a `count` argument.

**How to verify:**
```
cargo test
cargo run
# In Types mode with a long list: Ctrl-D jumps half a page down, Ctrl-U half up
# Near the end of the list: Ctrl-D clamps to last item
# Near the start: Ctrl-U clamps to first item
# Same behavior in Filters mode
```

### Task 3: Half-page jumps in fullscreen preview (`Ctrl-D` / `Ctrl-U`)

**ACs addressed:** AC8, AC9

**Files:**
- Modify: `src/tui/app.rs` (`handle_fullscreen_key`, `handle_key`)

**What to implement:**

Change `handle_fullscreen_key` signature to accept `KeyModifiers`:
```rust
fn handle_fullscreen_key(&mut self, code: KeyCode, modifiers: KeyModifiers)
```

Update the call site in `handle_key` (line 933) to pass `modifiers`.

Add match arms for `Ctrl-D` and `Ctrl-U`:
```rust
(KeyCode::Char('d'), KeyModifiers::CONTROL) => {
    self.scroll_offset = self.scroll_offset.saturating_add(self.fullscreen_height as u16 / 2);
}
(KeyCode::Char('u'), KeyModifiers::CONTROL) => {
    self.scroll_offset = self.scroll_offset.saturating_sub(self.fullscreen_height as u16 / 2);
}
```

This requires storing `fullscreen_height: usize` on `App`, set during `draw_fullscreen` in `ui.rs` (similar to `doc_list_height`). The fullscreen visible height is `area.height.saturating_sub(2)` (subtracting border).

Refactor `handle_fullscreen_key` to match on `(code, modifiers)` tuples.

AC9 (modal passthrough) is already handled: `handle_key` dispatches modals before fullscreen/normal, so `Ctrl-D`/`Ctrl-U` in modal states never reach the fullscreen or normal handlers.

**How to verify:**
```
cargo test
cargo run
# Open a long document in fullscreen (Enter)
# Ctrl-D scrolls down by half the visible height
# Ctrl-U scrolls back up
# Open a modal (n for create, d for delete) -- Ctrl-D/U should be ignored
```

## Test Plan

All tests go in a new `#[cfg(test)] mod tests` block at the bottom of `src/tui/app.rs`.

### T1: Viewport adjusts down with padding (AC1)
Set up an App with 20 doc_tree items, `doc_list_height = 10`, `selected_doc = 0`, `doc_list_offset = 0`. Call `move_down()` repeatedly until `selected_doc = 7`. Assert `doc_list_offset == 0` (selection still within padding). Call `move_down()` once more to `selected_doc = 8`. Assert `doc_list_offset == 1` (viewport scrolled to maintain 2-row bottom padding).

Property trade-offs: Behavioral over structure-insensitive (tests internal offset field, but this is the contract).

### T2: Viewport adjusts up with padding (AC2)
Set `doc_list_offset = 5`, `selected_doc = 7`, `doc_list_height = 10`. Call `move_up()` to `selected_doc = 6`. Assert `doc_list_offset == 4`. Continue to `selected_doc = 5`. Assert `doc_list_offset == 3`.

### T3: Sticky viewport on scroll-up (AC3)
Set `doc_list_offset = 5`, `selected_doc = 12`, `doc_list_height = 10`. Call `move_up()` to `selected_doc = 11`. Assert `doc_list_offset` stays at `5` (selection still within the viewport interior). Keep calling `move_up()` until `selected_doc = 7` and assert `doc_list_offset` is still `5` (first hits padding boundary).

### T4: Padding clamped at boundaries (AC4)
Set `selected_doc = 0`, `doc_list_offset = 0`, 5 items, `doc_list_height = 10`. Call `move_up()`. Assert `selected_doc == 0`, `doc_list_offset == 0`. Set `selected_doc` to last item. Call `move_down()`. Assert no change, no panic.

### T5: Half-page down (AC5)
Set `selected_doc = 0`, `doc_list_height = 10`, 20 items. Call `half_page_down()`. Assert `selected_doc == 5`. Assert `doc_list_offset` adjusted for padding.

### T6: Half-page up (AC6)
Set `selected_doc = 15`, `doc_list_height = 10`. Call `half_page_up()`. Assert `selected_doc == 10`.

### T7: Half-page clamping (AC7)
Set `selected_doc = 18`, 20 items, `doc_list_height = 10`. Call `half_page_down()`. Assert `selected_doc == 19` (clamped to last).

### T8: Fullscreen half-page (AC8)
Set `scroll_offset = 0`, `fullscreen_height = 20`. Call the equivalent of Ctrl-D. Assert `scroll_offset == 10`. Call Ctrl-U. Assert `scroll_offset == 0`.

### T9: Modal blocks half-page keys (AC9)
Already handled by dispatch order in `handle_key`. Verify by setting `create_form.active = true`, calling `handle_key` with Ctrl-D, asserting `selected_doc` and `scroll_offset` unchanged.

## Notes

The `draw_doc_list` and `draw_fullscreen` functions in `ui.rs` currently take `&App`. Task 1 and 3 require storing `doc_list_height` and `fullscreen_height` on App. The cleanest approach is to set these heights at the top of each draw function, which means changing the signature to `&mut App`. Alternatively, store them during a separate layout-calculation phase. The `&mut App` approach is simpler and matches how ratatui examples handle viewport-dependent state.
