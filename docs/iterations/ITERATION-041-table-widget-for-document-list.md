---
title: Table widget for document list
type: iteration
status: accepted
author: agent
date: 2026-03-08
tags: []
related:
- implements: STORY-047
---




## Changes

### Task 1: Extract shared row-building function

**ACs addressed:** AC-2 (shared rendering path), AC-4 (virtual label), AC-8 (tag overflow)

**Files:**
- Modify: `src/tui/ui.rs` (fn `doc_list_node_spans`, lines 170-241)

**What to implement:**

Extract a `doc_row_cells` function that returns a `Vec<Cell>` (ratatui table cells) instead of `Vec<Span>`. This function takes the common fields (id, title, status, tags, is_virtual, dim) and produces cells for: ID, title (with virtual suffix), status (colored), tags (first 3 as `[tag]`, overflow as `+N`).

This is the shared core that both Types mode and Filters mode will call. Types mode wraps it with an additional tree-prefix cell. Filters mode calls it directly (no tree column needed, but should still produce a tree cell with empty content for layout consistency).

Rename `doc_list_node_spans` to `doc_row_cells_for_node` -- it calls `doc_row_cells` and prepends the tree indicator cell.

The Filters mode inline span-building (ui.rs:874-912) gets replaced with a call to `doc_row_cells` that takes fields from the `Document` directly.

### Task 2: Replace List with Table in Types mode

**ACs addressed:** AC-1 (column alignment in Types mode), AC-3 (tree indicators), AC-5 (dim styling), AC-6 (selection highlight), AC-7 (column widths)

**Files:**
- Modify: `src/tui/ui.rs` (fn `draw_doc_list`, lines 243-287)

**What to implement:**

Replace the `List` widget with a ratatui `Table` widget. Define column constraints:

| Column | Constraint |
|--------|-----------|
| Tree | `Constraint::Length(4)` |
| ID | `Constraint::Length(14)` |
| Title | `Constraint::Fill(1)` |
| Status | `Constraint::Length(12)` |
| Tags | `Constraint::Min(20)` |

Build rows using `doc_row_cells_for_node` from Task 1. Each `DocListNode` produces a `Row` of `Cell`s.

Replace `ListState` with `TableState` for selection tracking. Keep the same highlight logic:
- Normal: `Modifier::REVERSED`
- Relations focused: `DarkGray` + `Modifier::BOLD`

Keep the same dim styling: when Relations tab is focused, all row styles use `DarkGray`.

Preserve border styling (Cyan when focused, DarkGray when Relations focused).

### Task 3: Replace List with Table in Filters mode

**ACs addressed:** AC-2 (shared rendering path, identical layout)

**Files:**
- Modify: `src/tui/ui.rs` (fn `draw_filters_mode`, lines 808-937)

**What to implement:**

Replace the inline `ListItem` building (lines 874-912) and `List` widget (lines 926-937) with a `Table` using the same column constraints from Task 2.

For each filtered doc, call `doc_row_cells` with an empty tree cell (Filters mode is flat, no hierarchy). Use `TableState` instead of `ListState`.

The column constraints, highlight style, and border style logic should be identical to Types mode. Extract these as constants or a shared helper if the duplication is non-trivial (e.g. a `doc_table_constraints()` function returning the `Vec<Constraint>`).

### Task 4: Update app state for TableState

**ACs addressed:** AC-6 (selection highlight via TableState)

**Files:**
- Modify: `src/tui/app.rs` (wherever `ListState` is referenced for doc list selection)

**What to implement:**

If `draw_doc_list` and `draw_filters_mode` currently create `ListState` inline from `app.selected_doc`, the same pattern works with `TableState`. Check whether any scroll/viewport state needs updating. `TableState::default().with_selected(Some(app.selected_doc))` should be the drop-in replacement.

Verify that `selected_doc` navigation (j/k, arrow keys, expand/collapse) still works correctly with the Table widget's selection model.

**How to verify:**
- `cargo run` and navigate the TUI, confirm selection moves correctly
- Expand/collapse parent nodes, verify selection stays consistent

## Test Plan

### T1: Row cell generation for standard document
Create a `DocListNode` with known fields (id="RFC-001", title="Test", status=Draft, tags=["a","b"], is_virtual=false, dim=false). Call `doc_row_cells` and assert the returned cells contain the expected text content and styles. Verifies AC-1 (column content), AC-4 (no virtual label), AC-8 (tags under limit).

### T2: Row cell generation for virtual document
Same as T1 but with `is_virtual=true`. Assert the title cell contains "(virtual)" suffix. Verifies AC-4.

### T3: Tag overflow rendering
Create a node with 5 tags. Assert the tags cell shows first 3 as `[tag]` and ends with `+2`. Verifies AC-8.

### T4: Dim styling when Relations focused
Call `doc_row_cells` with `dim=true`. Assert all cells use `DarkGray` foreground. Verifies AC-5.

### T5: Tree indicator cells
Call `doc_row_cells_for_node` with various node configurations:
- Root parent expanded: cell contains `▼`
- Root parent collapsed: cell contains `▶`
- Child (not last): cell contains `├─`
- Child (last): cell contains `└─`
Verifies AC-3.

### T6: Visual smoke test (manual)
Run `cargo run` with a project that has documents of varying ID lengths, titles, tag counts. Confirm columns align in both Types and Filters mode. Toggle Relations tab, confirm dim styling. This is the primary verification for AC-1, AC-2, AC-5, AC-6, AC-7.

> [!NOTE]
> T1-T5 are unit tests in `src/tui/ui.rs::tests`. T6 is manual. The unit tests trade Predictive for Fast -- they verify cell content but not visual layout. T6 covers the visual gap.

## Notes

Column widths change from current values (ID was 12, now 14; title was 20 fixed, now flexible fill; tags had no minimum, now 20 min). This is intentional per the Story spec.

The Filters mode currently shows `display_name` (filename-derived) rather than the separate ID + title columns. The shared rendering should use the same ID + title columns as Types mode for layout consistency (AC-2 says "identical layout"). Filters mode doesn't have tree hierarchy, so the tree column will be empty but still present to maintain alignment.
