---
title: Minimal Filters Mode
type: iteration
status: accepted
author: agent
date: 2026-03-06
tags: []
related:
- implements: docs/stories/STORY-013-filters-mode.md
---



## Design Decisions

- **Interaction model:** j/k always controls the doc list (right panel). Tab/Shift-Tab moves between filter fields in the left panel. h/l cycles filter values.
- **Scope:** Status and Tag filters only. No author, no sort.
- **Persistence:** Filters reset when leaving Filters mode. No cross-mode state.
- **Filter values:** Dynamically collected from the store on entering Filters mode.
- **Doc list in Filters mode:** Shows all doc types (no type filter). The Store::Filter already supports status and tag; we pass `doc_type: None`.

## ACs Addressed

From STORY-013: AC1 (filter controls panel), AC2 (navigate filter fields), AC3 (cycle filter values), AC4 (real-time filtering), AC5 (filtered count display), AC6 (clear filters), AC8 (preview and relations work with filtered list).

AC7 (filters persist across mode switches) is intentionally excluded per design decision above.

## Changes

### Task 1: Add filter state to App

**ACs addressed:** AC1, AC3, AC6

**Files:**
- Modify: `src/tui/app.rs`

**What to implement:**

Add a `FilterField` enum with variants `Status` and `Tag`, plus a `next`/`prev` cycle that wraps around (including a `ClearAction` variant as the third position).

Add filter state fields to the `App` struct:

```
filter_focused: FilterField       // which field has focus
filter_status: Option<Status>     // None means "all"
filter_tag: Option<String>        // None means "all"
available_tags: Vec<String>       // populated on entering Filters mode
```

Initialize all to defaults in `App::new`. Add a method `enter_filters_mode(&mut self)` that collects unique tags from `self.store.all_docs()`, sorts them, and stores in `available_tags`. Call this from `cycle_mode` when entering `ViewMode::Filters`.

Add a method `filtered_docs(&self) -> Vec<&DocMeta>` that calls `self.store.list()` with a `Filter` built from `filter_status` and `filter_tag` (with `doc_type: None`), sorted by path.

Add a method `reset_filters(&mut self)` that sets `filter_status` to None, `filter_tag` to None, and `filter_focused` to `FilterField::Status`.

Add methods `cycle_filter_value_next(&mut self)` and `cycle_filter_value_prev(&mut self)` that cycle the value of the currently focused filter field. For Status: None -> Draft -> Review -> Accepted -> Rejected -> Superseded -> None. For Tag: None -> each tag in available_tags -> None. For ClearAction: no-op (h/l does nothing on the clear action).

**How to verify:**
```
cargo test
```

### Task 2: Add filter key handling

**ACs addressed:** AC2, AC3, AC4, AC6, AC8

**Files:**
- Modify: `src/tui/app.rs`

**What to implement:**

In `handle_normal_key`, add a `ViewMode::Filters` block before the existing match statement (following the Graph mode pattern). The block handles:

- `Tab` -> `self.filter_focused = self.filter_focused.next()`
- `BackTab` (Shift-Tab) -> `self.filter_focused = self.filter_focused.prev()`
- `h` / `Left` -> `self.cycle_filter_value_prev()`
- `l` / `Right` -> `self.cycle_filter_value_next()`
- `Enter` when `filter_focused == FilterField::ClearAction` -> `self.reset_filters()`
- `j` / `Down` -> navigate filtered doc list down (reuse `move_down` but operate on `filtered_docs().len()` instead of `docs_for_current_type().len()`)
- `k` / `Up` -> navigate filtered doc list up
- `Enter` (not on ClearAction) -> fullscreen or navigate relation (same as Types mode)
- `g` / `G` -> top/bottom of filtered doc list
- `e` -> open editor for selected filtered doc
- `q` / Ctrl+C -> quit
- Backtick -> cycle mode
- `?` -> help
- `/` -> search
- `Tab` for preview tab toggle needs consideration: since Tab is used for filter fields, use a different approach. The preview tab toggle can be triggered when `preview_tab == PreviewTab::Relations` context isn't needed in this iteration, or we keep Tab for filter fields and don't toggle preview tabs in Filters mode.

Add a helper `selected_filtered_doc(&self) -> Option<&DocMeta>` that returns the doc at `self.selected_doc` index from `filtered_docs()`.

Update `cycle_mode` to call `self.reset_filters()` when leaving Filters mode (i.e., when the current mode is Filters and we're cycling away). Also call `self.enter_filters_mode()` when entering Filters mode. Reset `selected_doc` to 0 when entering Filters mode.

**How to verify:**
```
cargo test
```

### Task 3: Render Filters mode UI

**ACs addressed:** AC1, AC3, AC4, AC5, AC8

**Files:**
- Modify: `src/tui/ui.rs`

**What to implement:**

Replace the `draw_filters_skeleton` call in the `draw` function's match arm with a new `draw_filters_mode(f, app, area)` function. Delete `draw_filters_skeleton`.

`draw_filters_mode` layout:
- Horizontal split: 20% left (filter controls), 80% right
- Right side: vertical split for doc list (60%) and preview/relations (40%), same as Types mode

Left panel rendering:
- Title: " Filters "
- Render each filter field as a line: `"  Status: [value]"` and `"  Tag: [value]"` and `"  [clear filters]"`
- The focused field gets a highlight style (bold, cyan foreground)
- The value shows "all" when the filter is None, or the actual value when set
- Active (non-None) filter values render in yellow to indicate they're filtering

Right panel rendering:
- Reuse the existing doc list rendering pattern from Types mode, but operate on `app.filtered_docs()` instead of `app.docs_for_current_type()`
- Title: `format!(" Documents ({} of {}) ", filtered_count, total_count)` where total_count is `app.store.all_docs().len()`
- Selection highlight uses `app.selected_doc` index
- Below the doc list, render preview/relations panel for the selected filtered doc (reuse `draw_preview` and `draw_relations` helpers if they exist, or replicate the pattern from Types mode)

**How to verify:**
```
cargo run
```
Then press backtick to cycle to Filters mode. Verify the left panel shows filter fields, the right panel shows all documents, and Tab/h/l/j/k work as expected.

### Task 4: Tests for filter state and interaction

**ACs addressed:** AC1-AC6, AC8

**Files:**
- Create: `tests/tui_filters_test.rs`

**What to implement:**

Follow the existing test pattern from `tests/tui_graph_test.rs` and `tests/tui_view_mode_test.rs`. Use `TestFixture` from `tests/common/mod.rs`.

Create test fixtures with documents that have varied statuses and tags to exercise filtering. Tests to write:

1. `test_entering_filters_mode_collects_tags` - Create docs with different tags, cycle to Filters mode, assert `available_tags` contains the expected sorted unique tags.

2. `test_filter_field_navigation` - Enter Filters mode, press Tab repeatedly, assert `filter_focused` cycles through Status -> Tag -> ClearAction -> Status.

3. `test_cycle_status_filter` - Enter Filters mode, press `l` to cycle status values, assert `filter_status` progresses through None -> Draft -> Review -> ... -> None.

4. `test_cycle_tag_filter` - Enter Filters mode, Tab to Tag field, press `l`, assert `filter_tag` cycles through available tags.

5. `test_filtered_docs_returns_matching` - Set `filter_status` to Some(Status::Draft), call `filtered_docs()`, assert only draft docs are returned.

6. `test_combined_filters` - Set both status and tag filters, assert `filtered_docs()` returns only docs matching both.

7. `test_clear_filters` - Set filters, Tab to ClearAction, press Enter, assert both filters reset to None.

8. `test_doc_navigation_in_filters_mode` - Enter Filters mode, press j/k, assert `selected_doc` changes within the filtered list bounds.

9. `test_filters_reset_on_mode_switch` - Set filters in Filters mode, press backtick to leave, press backtick to return, assert filters are reset.

10. `test_enter_opens_fullscreen_for_filtered_doc` - Enter Filters mode, select a doc with j, press Enter, assert fullscreen is active.

**How to verify:**
```
cargo test tui_filters
```

## Test Plan

Tests are described in Task 4. Key properties:

| Test | Key Properties |
|------|---------------|
| tag collection | Deterministic, Behavioral - verifies store data extraction |
| field navigation | Isolated, Fast - pure state transitions |
| value cycling | Isolated, Deterministic - enum cycling with wraparound |
| filtered docs | Behavioral, Predictive - tests the core filtering contract |
| combined filters | Predictive - AND logic matches Store::list behavior |
| clear filters | Specific - one action, one expected outcome |
| doc navigation | Structure-insensitive - tests bounds, not rendering |
| mode switch reset | Behavioral - tests the "no persistence" design decision |
| fullscreen | Composable - works regardless of filter state |

All tests use `TestFixture` for setup and operate on `App` state directly (no terminal rendering needed). This keeps them Fast and Isolated.

## Notes

- The `Store::Filter` struct already supports the filtering we need. No store changes required.
- Status enum values are hardcoded (Draft, Review, Accepted, Rejected, Superseded) so the cycle order is deterministic.
- Tag values are dynamic and collected from the store, so the cycle order depends on what docs exist.
- Preview/Relations tab toggle (currently Tab) is not available in Filters mode since Tab is used for filter field navigation. This is acceptable for the minimal version.
