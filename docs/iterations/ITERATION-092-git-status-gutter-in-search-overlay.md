---
title: Git status gutter in search overlay
type: iteration
status: accepted
author: agent
date: 2026-03-23
tags: []
related:
- implements: STORY-078
---



## Changes

### Task 1: Render gutter indicator in search overlay list items

ACs addressed: AC-1, AC-2, AC-3, AC-5, AC-6

Files:
- Modify: `src/tui/ui.rs`

What to implement:

In `draw_search_overlay` (around line 1419), update the `items` iterator to prepend a gutter span to each `Line`. Before the existing `Span::raw(format!(" {:<40} ", title))`, insert a gutter span derived from `app.git_status_cache.get(path)`:

- `Some(GitFileStatus::New)` → `Span::styled("┃", Style::default().fg(Color::Green))`
- `Some(GitFileStatus::Modified)` → `Span::styled("┃", Style::default().fg(Color::Yellow))`
- `None` → `Span::raw(" ")`

The `git_status_cache` import (`GitFileStatus`) is already in scope at the top of `ui.rs` from Task 3 of ITERATION-091.

The resulting `Line::from(vec![gutter_span, title_span, status_span])` should replace the existing two-element vec.

The `refresh()` call at the top of `draw()` (line 128) ensures the cache is current for every render, so no additional refresh logic is needed.

How to verify: `cargo run` — open the TUI, enter search mode with `/`, type a query. Documents with git changes should show `┃` in green or yellow at the left of each result row. Documents without changes show a blank space.

---

### Task 2: Tests for search overlay ACs

ACs addressed: AC-1, AC-2, AC-3, AC-5

Files:
- Modify: `tests/tui_git_status.rs`

What to implement:

Add three tests to `tests/tui_git_status.rs`, following the pattern of existing tests in that file. The tests drive `App` state and assert on `git_status_cache` lookups (the same cache that `draw_search_overlay` reads).

1. `test_search_results_new_file_has_green_indicator` — create a fixture with a git repo, add an untracked doc file, build `App`, populate `app.search_results` with that file's path, assert `app.git_status_cache.get(&path)` returns `Some(GitFileStatus::New)`.

2. `test_search_results_modified_file_has_yellow_indicator` — commit a doc, modify it, build `App`, populate `app.search_results` with the path, assert `app.git_status_cache.get(&path)` returns `Some(GitFileStatus::Modified)`.

3. `test_search_results_unchanged_file_no_indicator` — commit a doc without modifying, build `App`, populate `app.search_results` with the path, assert `app.git_status_cache.get(&path)` returns `None`.

These tests assert on cache state rather than rendered output, which is structure-insensitive and avoids coupling to terminal widget internals. The tradeoff is they do not verify the gutter span color — that remains a visual verification step (Task 1's "how to verify").

For AC-5 (consistency across view switches): cache state is identical regardless of which view is active, because `refresh()` runs once per render in `draw()` and all views read the same `App`. This is verified implicitly by the cache tests above.

How to verify: `cargo test tui_git_status`

## Test Plan

| AC | Test | Properties |
|----|------|------------|
| AC-1 (green for new in search) | `test_search_results_new_file_has_green_indicator` | Isolated, deterministic, behavioral |
| AC-2 (yellow for modified in search) | `test_search_results_modified_file_has_yellow_indicator` | Isolated, deterministic, behavioral |
| AC-3 (no indicator for unchanged in search) | `test_search_results_unchanged_file_no_indicator` | Isolated, deterministic, behavioral |
| AC-5 (consistency across views) | Covered implicitly — single shared cache, refreshed once per `draw()` | N/A |
| AC-4 (filtered list gutter) | Covered by ITERATION-091 | N/A |
| AC-6 (outside git repo) | Covered by `test_non_git_repo` in ITERATION-091 | N/A |

Tests 1–3 invoke real git commands in a temp dir (sacrificing speed for predictability). This matches the pattern established in ITERATION-091.

## Notes

- The filtered list (`draw_filters_mode`) already has gutter rendering from ITERATION-091 — AC-4 is fully satisfied without any changes here.
- AC-5 is structurally satisfied by the single `app.git_status_cache.refresh()` call at the top of `draw()`, which runs before any view renders.
- AC-6 (no indicator outside git repo) is covered by `test_non_git_repo` in the existing test file.
- The search overlay renders a `List` widget, not a `Table`. The gutter is a leading span in each `ListItem`, not a dedicated column — this matches the RFC's intent ("single-character-width gutter") without requiring a Table conversion.
