---
title: Scrollbar widget for focused views
type: iteration
status: accepted
author: agent
date: 2026-03-08
tags: []
related:
- implements: docs/stories/STORY-049-scrollbar-for-focused-views.md
---



## Context

Add ratatui's `Scrollbar` widget to the three scrollable views in the TUI: document list, fullscreen preview, and relations list. The scrollbar only renders when the view is focused and content overflows. Styled with a `DarkGray` track and `Cyan` thumb to match existing focus styling.

Currently no `Scrollbar` or `ScrollbarState` is used anywhere in the codebase. The three views use different scroll mechanisms: `TableState` with `doc_list_offset` for the doc list, `Paragraph.scroll()` with `scroll_offset` for fullscreen, and `ListState` for relations.

## Changes

### Task 1: Add scrollbar to the document list view

**ACs addressed:** AC-1 (scrollbar when focused and overflows), AC-2 (no scrollbar when dimmed), AC-5 (no scrollbar when content fits), AC-6 (styling), AC-7 (correct ScrollbarState)

**Files:**
- Modify: `src/tui/ui.rs` (function `draw_doc_list`, ~line 270)

**What to implement:**

After rendering the `Table` widget in `draw_doc_list()`, conditionally render a `Scrollbar` widget on the right edge of the content area.

1. Import `Scrollbar`, `ScrollbarOrientation`, `ScrollbarState` from ratatui
2. After the existing `StatefulWidget::render(table, ...)` call (~line 310):
   - Guard: only render when `!dim` (not relations-focused) AND total item count > visible height (`app.doc_list_height`)
   - Create `ScrollbarState::new(total_items).position(app.doc_list_offset)`
   - Create `Scrollbar::new(ScrollbarOrientation::VerticalRight)` with `.track_style(Style::default().fg(Color::DarkGray))` and `.thumb_style(Style::default().fg(Color::Cyan))`
   - Render the scrollbar into the same `area` (it positions itself on the right edge)

The total item count is the length of the flattened document list. `app.doc_list_offset` already tracks the viewport position. `app.doc_list_height` tracks visible height (set at line 271).

**How to verify:**
- `cargo test` passes
- Manual: open TUI with enough documents to overflow, confirm scrollbar appears on right edge
- Switch focus to relations tab, confirm scrollbar disappears from doc list
- With few documents (no overflow), confirm no scrollbar

### Task 2: Add scrollbar to the fullscreen preview

**ACs addressed:** AC-3 (scrollbar in fullscreen, thumb tracks offset), AC-5 (no scrollbar when content fits), AC-6 (styling), AC-7 (correct ScrollbarState)

**Files:**
- Modify: `src/tui/ui.rs` (function `draw_fullscreen`, ~line 501)

**What to implement:**

After rendering the fullscreen `Paragraph` in `draw_fullscreen()`, conditionally render a `Scrollbar`.

1. After the `Paragraph` render (~line 543):
   - Determine total content lines. The fullscreen content is built from `wrapped_lines` (or the paragraph's line count). The total line count needs to be calculated from the rendered content. Use the line count of the text content being displayed.
   - Guard: only render when total lines > `app.fullscreen_height`
   - Create `ScrollbarState::new(total_lines).position(app.scroll_offset as usize)`
   - Same `Scrollbar` styling as Task 1 (`DarkGray` track, `Cyan` thumb)
   - Render into the content area

Fullscreen is always "focused" when visible, so no focus guard needed beyond the overflow check.

**How to verify:**
- `cargo test` passes
- Manual: open fullscreen on a long document, scroll with `j`/`k`, confirm scrollbar thumb moves
- Open fullscreen on a short document, confirm no scrollbar

### Task 3: Add scrollbar to the relations list

**ACs addressed:** AC-4 (scrollbar when relations focused and overflows), AC-5 (no scrollbar when content fits), AC-6 (styling), AC-7 (correct ScrollbarState)

**Files:**
- Modify: `src/tui/ui.rs` (function `draw_relations_content`, ~line 416)

**What to implement:**

After rendering the relations `List` in `draw_relations_content()`, conditionally render a `Scrollbar`.

1. After the `StatefulWidget::render(list, ...)` call (~line 498):
   - The relations list is only visible when `app.preview_tab == PreviewTab::Relations`, which means it's focused by definition (the doc list is dimmed instead)
   - Guard: only render when total list items > visible area height
   - Total items: use the flat item count from the list (including category headers)
   - Position: derive from `selected_flat_index` or the list's scroll state
   - Create `ScrollbarState::new(total_items).position(current_offset)`
   - Same `Scrollbar` styling as Tasks 1 and 2
   - Render into the relations content area

**How to verify:**
- `cargo test` passes
- Manual: create enough relations to overflow, focus on relations tab, confirm scrollbar appears
- With few relations (no overflow), confirm no scrollbar

### Task 4: Extract shared scrollbar helper

**ACs addressed:** Supports all ACs (reduces duplication across Tasks 1-3)

**Files:**
- Modify: `src/tui/ui.rs`

**What to implement:**

After Tasks 1-3 are working, extract a small helper to reduce duplication:

```rust
fn render_scrollbar(area: Rect, buf: &mut Buffer, total: usize, position: usize)
```

This function creates the `Scrollbar` and `ScrollbarState` with the standard styling (`DarkGray` track, `Cyan` thumb) and renders it. Replace the inline scrollbar code in all three views with calls to this helper.

Only extract if the three call sites are clearly duplicating the same 4-5 lines. If differences between views make a shared helper awkward, skip this task.

**How to verify:**
- `cargo test` passes
- Behavior unchanged from Tasks 1-3

## Test Plan

### Test 1: Scrollbar renders when doc list overflows and is focused

Render the doc list view with more items than `doc_list_height`. Assert the rendered buffer contains scrollbar characters on the right edge of the content area. This is a unit-level render test using ratatui's `TestBackend`.

- **Tradeoff:** Structure-sensitive (depends on exact character positions), but necessary to verify visual output.

### Test 2: Scrollbar hidden when doc list is dimmed

Render the doc list with `preview_tab` set to `Relations` (dimmed state). Assert no scrollbar characters appear in the doc list area, even with overflowing content.

### Test 3: Scrollbar hidden when content fits

Render the doc list with fewer items than `doc_list_height`. Assert no scrollbar characters appear.

### Test 4: Fullscreen scrollbar tracks scroll offset

Render fullscreen preview with long content. Set `scroll_offset` to various positions. Assert scrollbar thumb position changes accordingly.

### Test 5: Relations scrollbar renders when focused and overflows

Render the relations list with enough items to overflow. Assert scrollbar appears on the right edge.

## Notes

- ratatui's `Scrollbar` widget is part of the `widgets` module, available since ratatui 0.26. The project uses ratatui 0.30, so no dependency changes needed.
- The `Scrollbar` widget renders into the same `Rect` as the content area -- it automatically positions on the specified edge. No layout changes required.
- Relations list currently has no viewport offset tracking (just selection). `ScrollbarState` can use the `ListState` offset or derive position from the selected index relative to total items.
