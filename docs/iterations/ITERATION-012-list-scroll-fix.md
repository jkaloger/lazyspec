---
title: List Scroll Fix
type: iteration
status: accepted
author: agent
date: 2026-03-05
tags: []
related:
- implements: STORY-003
---




## Problem

All four `List` widgets in the TUI use `render_widget` (stateless), which always renders from item 0. When the selected index moves past the visible area, the highlight disappears below the fold. The fix is to use ratatui's `ListState` with `render_stateful_widget`, which automatically scrolls to keep the selected item visible.

Affected lists:
- Document list (`draw_doc_list`, ui.rs:186-193)
- Type panel (`draw_type_panel`, ui.rs:117-123)
- Relations list (`draw_relations_content`, ui.rs:384-385)
- Search results (`draw_search_overlay`, ui.rs:640-646)

## Changes

### Task 1: Use ListState for the document list

**Files:**
- Modify: `src/tui/ui.rs` (draw_doc_list function, lines 126-194)

**What to implement:**

In `draw_doc_list`:
1. Import `ListState` from `ratatui::widgets`
2. Remove the manual `REVERSED` style on the selected item (ListState handles highlight)
3. Add `.highlight_style(Style::default().add_modifier(Modifier::REVERSED))` to the `List` builder (and a dimmed variant when `relations_focused`)
4. Create a `ListState::default().with_selected(Some(app.selected_doc))`
5. Replace `f.render_widget(list, area)` with `f.render_stateful_widget(list, area, &mut state)`

The dim styling when relations are focused should be preserved on the highlight style.

**How to verify:**
Run the TUI with a doc type that has more documents than fit on screen. Press `j` repeatedly past the fold. The list should scroll to keep the selected item visible.

### Task 2: Use ListState for the type panel

**Files:**
- Modify: `src/tui/ui.rs` (draw_type_panel function, lines 98-124)

**What to implement:**

In `draw_type_panel`:
1. Remove the manual bold/cyan style on the selected item
2. Add `.highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))` to the `List`
3. Create a `ListState::default().with_selected(Some(app.selected_type))`
4. Replace `f.render_widget(list, area)` with `f.render_stateful_widget(list, area, &mut state)`

This panel is unlikely to overflow (only 4 types), but consistency matters and it's trivial to fix alongside the others.

**How to verify:**
Run the TUI, switch types with `h`/`l`. Highlight should still work identically.

### Task 3: Use ListState for search results

**Files:**
- Modify: `src/tui/ui.rs` (draw_search_overlay function, lines 592-647)

**What to implement:**

In `draw_search_overlay`:
1. Remove the manual `REVERSED` style on the selected search result
2. Add `.highlight_style(Style::default().add_modifier(Modifier::REVERSED))` to the `List`
3. Create a `ListState::default().with_selected(Some(app.search_selected))`
4. Replace `f.render_widget(list, layout[1])` with `f.render_stateful_widget(list, layout[1], &mut state)`

**How to verify:**
Open search with `/`, type a query that returns many results. Navigate past the fold with `j`/down arrow. Results list should scroll.

### Task 4: Use ListState for relations list

**Files:**
- Modify: `src/tui/ui.rs` (draw_relations_content function, lines 298-386)

**What to implement:**

The relations list is trickier because it mixes group headers (non-selectable) with selectable items. `ListState` tracks a flat index across all items, but `app.selected_relation` only counts selectable items.

Approach: track the flat index (including headers) that corresponds to `app.selected_relation`. While building the items list, maintain a mapping from selectable-index to flat-index. Then set `ListState::selected` to the flat index.

1. While iterating, track `flat_index_in_list` (incremented for every item including headers) alongside the existing `flat_index` (selectable items only)
2. When `flat_index == app.selected_relation`, record `flat_index_in_list` as the selected flat index
3. Remove the manual `> ` indicator and cyan styling on the selected item
4. Add `.highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))` and `.highlight_symbol("  > ")` to the `List`
5. Create `ListState::default().with_selected(Some(selected_flat_index))`
6. Replace `f.render_widget(list, area)` with `f.render_stateful_widget(list, area, &mut state)`

**How to verify:**
Open a document with many relations. Switch to Relations tab with `Tab`. Navigate with `j`/`k`. The selected relation should stay visible.

## Test Plan

This is a rendering-only change with no logic changes to selection tracking. The existing `App` methods (`move_up`, `move_down`, etc.) remain unchanged.

**Manual verification** (TUI rendering is not unit-testable without a terminal backend):

1. Document list scroll: navigate to Iterations (most docs), press `j` until past the fold. List should scroll. Press `k` back up. Press `G` to jump to bottom, `g` to jump to top.
2. Type panel: switch types with `h`/`l`. Highlight should work as before.
3. Search results scroll: `/` to search, type a broad query, navigate results past the fold.
4. Relations scroll: select a doc with relations, `Tab` to Relations, navigate with `j`/`k`.

No automated tests are planned. The change is purely presentational, swapping `render_widget` for `render_stateful_widget`. The selection logic is unchanged.

## Notes

`ListState` is a standard ratatui pattern for scrollable lists. It manages an internal `offset` field that tracks the first visible item, automatically adjusting when the selected item would be off-screen. This is the idiomatic way to handle list scrolling in ratatui.
