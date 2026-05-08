---
title: Title prioritised in TUI tables at narrow widths
type: iteration
status: draft
author: agent
date: 2026-05-08
tags: []
related:
- implements: STORY-047
---

## Context

`doc_table_widths()` in `src/tui/views/panels.rs:216` returns a fixed 7-column constraint set:

```
gutter(Length 1), tree(Length 4), id(Length 18), title(Fill 1),
status(Length 12), tags(Length 24), provenance(Min 20)
```

Fixed-length columns sum to `1 + 4 + 18 + 12 + 24 = 59` plus 6 column spacings = `65`. With provenance `Min(20)` and panel borders (2 cols), title's `Fill(1)` only receives a positive allocation when terminal width exceeds ~`87`. Below that, ratatui collapses Fill to 0 and clips the title.

`doc_table_widths()` is reused by:

1. `draw_doc_list` (Types mode) at `panels.rs:729`
2. Filters mode table at `panels.rs:1311`

Both pass `area.width` further down to `doc_row_for_node`, so width is already in scope at call sites. `DocCellWidths::from_area_width` (`panels.rs:278`) mirrors `doc_table_widths()` for soft-wrap measurement and must agree with the constraint set.

## Goal

Title column receives priority allocation at all terminal widths. Less-critical columns (provenance, then tags, then status) collapse first; title retains a configured minimum. ID, gutter and tree never collapse (they identify the row).

## Approach

Convert `doc_table_widths()` to `doc_table_widths(area_width: u16) -> [Constraint; 7]`. Compute remaining budget after fixed essentials (gutter, tree, ID), then allocate optional columns by descending priority. Title uses `Constraint::Min(TITLE_MIN)` so ratatui guarantees the minimum and any surplus flows there.

Breakpoints (terminal columns inside table block):

| Budget after gutter+tree+ID+spacings | Visible optional columns        |
|--------------------------------------|---------------------------------|
| `>= TITLE_MIN + 12 + 24 + 20`        | status, tags, provenance        |
| `>= TITLE_MIN + 12 + 24`             | status, tags (provenance hidden) |
| `>= TITLE_MIN + 12`                  | status (tags, provenance hidden) |
| else                                  | title only (status hidden)      |

`TITLE_MIN = 20`. Hidden columns use `Constraint::Length(0)`. Row cell vector length stays 7 so existing `Row::new(cells)` code is unchanged. `DocCellWidths::from_area_width` calls the new function with the same `area_width` so wrap math stays in sync.

## Changes

1. **Refactor `doc_table_widths` to take area width.** File: `src/tui/views/panels.rs`. Replace `fn doc_table_widths() -> [Constraint; 7]` (line 216) with `fn doc_table_widths(area_width: u16) -> [Constraint; 7]`. Define module-level constants `TITLE_MIN_COLS: u16 = 20`, `STATUS_COLS: u16 = 12`, `TAGS_COLS: u16 = 24`, `PROV_MIN_COLS: u16 = 20`, `ID_COLS: u16 = 18`, `TREE_COLS: u16 = 4`, `GUTTER_COLS: u16 = 1`. Compute `fixed = GUTTER_COLS + TREE_COLS + ID_COLS + 6 /* column_spacing */`. Subtract from `area_width.saturating_sub(2)` (block borders) to get `budget`. Return `[Length(GUTTER_COLS), Length(TREE_COLS), Length(ID_COLS), Min(TITLE_MIN_COLS), Length(status), Length(tags), Min(prov)]` where `status`, `tags`, `prov` are 0 when budget thresholds aren't met. Verifies: AC-1, AC-2, AC-3.

2. **Update `DocCellWidths::from_area_width`.** File: `src/tui/views/panels.rs:278`. Pass `area_width` through to the new `doc_table_widths(area_width)` call. The split rect logic stays; doc comment updated to reflect responsive behaviour. Verifies: AC-4.

3. **Update both call sites to pass area width.** File: `src/tui/views/panels.rs`.
   - `draw_doc_list` (line 729): replace `let widths = doc_table_widths();` with `let widths = doc_table_widths(area.width);`.
   - Filters mode panel (line 1311): replace `let widths = doc_table_widths();` with `let widths = doc_table_widths(right[0].width);` (the right-pane Rect is in scope, see surrounding code at `panels.rs:1343`).
   Verifies: AC-1, AC-2.

4. **Add unit tests.** File: `src/tui/views/panels.rs` (existing `#[cfg(test)] mod tests`). Tests cover constraint resolution at representative widths by splitting a `Rect::new(0,0,w-2,1)` with the returned constraints (matching `DocCellWidths::from_area_width`'s technique). Cases below. Verifies: AC-1..AC-5.

## Test Plan

Tests are unit tests in `src/tui/views/panels.rs::tests`. They drive `doc_table_widths(width)` and resolve the rect split, then assert on cell widths. This is structure-insensitive (uses ratatui's own `Layout::split` to compute the realised cell widths) and behavioural (asserts on the rendered allocation, not internal arithmetic).

| Test name                                              | Scenario                                | Assertion                                                                  |
|--------------------------------------------------------|-----------------------------------------|----------------------------------------------------------------------------|
| `doc_table_widths_wide_shows_all_columns`              | `width = 200`                           | title `>= 20`; status `= 12`; tags `= 24`; provenance `>= 20`              |
| `doc_table_widths_medium_drops_provenance`             | `width = 90` (below provenance budget)  | provenance `= 0`; tags `= 24`; status `= 12`; title `>= 20`                |
| `doc_table_widths_narrow_drops_tags_and_provenance`    | `width = 70`                            | tags `= 0`; provenance `= 0`; status `= 12`; title `>= 20`                 |
| `doc_table_widths_very_narrow_drops_status`            | `width = 50`                            | status `= 0`; tags `= 0`; provenance `= 0`; title `>= 20` or remaining     |
| `doc_cell_widths_match_constraint_split`               | `width = 80`                            | `DocCellWidths::from_area_width(80).title` equals `doc_table_widths(80)` resolved title |
| `doc_table_widths_preserves_id_and_tree_at_all_widths` | `width = 40, 60, 80, 120`               | ID column always `= 18`; tree always `= 4`; gutter always `= 1`            |

AC-3 (selected-row highlight) and AC-2 (Filters mode parity) are reachable through the existing TUI integration tests and verified by manual smoke. No new selection-behaviour tests are required since the constraint refactor preserves cell ordering.

### Tradeoffs

- Asserting on resolved rect widths (via `Layout::split`) couples the tests to ratatui's allocator. This is acceptable: `DocCellWidths::from_area_width` already takes that dependency for soft-wrap measurement, and using the same allocator means the test verifies what ratatui will actually render.
- We could instead inspect the returned `[Constraint; 7]` directly. That is structure-coupled (asserts on `Constraint::Length(x)` literals) and would not catch interaction bugs between constraints. Rejected.

## Acceptance Criteria

- **AC-1** Given a wide terminal (>= 120 columns) when the document list renders then all columns (gutter, tree, ID, title, status, tags, provenance) are visible at their nominal widths.
- **AC-2** Given a narrow terminal (~70 columns) when the document list renders then provenance and tags are hidden (width 0), status remains at 12, and title receives at least `TITLE_MIN_COLS` (20).
- **AC-3** Given a very narrow terminal (~50 columns) when the document list renders then status, tags, and provenance are hidden, and title takes the remaining budget.
- **AC-4** Given any terminal width when soft-wrap measurement runs then `DocCellWidths::from_area_width(w).title` equals the title cell width that ratatui resolves from `doc_table_widths(w)`.
- **AC-5** Given any terminal width when the document list renders then gutter (1), tree (4), and ID (18) columns retain their fixed widths.

## Notes

- The 6-cell column-spacing total comes from ratatui's default `column_spacing = 1` between 7 adjacent cells.
- ID column at 18 cols accommodates the `[gh]` badge appended in `doc_row_cells` (`panels.rs:454`). Do not shrink ID below 18 in this iteration; that is a separate display change.
- Convention DICTUM-004 (testing): tests live alongside the module, no sleeps, deterministic, behavioural. Constraint-split assertions satisfy this.
- STORY-047 mandates ID column at 14, but the codebase already drifted to 18 (predates this iteration). Out of scope to reconcile.
