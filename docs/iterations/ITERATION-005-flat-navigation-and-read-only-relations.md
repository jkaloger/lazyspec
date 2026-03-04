---
title: Flat Navigation and Read-Only Relations
type: iteration
status: accepted
author: agent
date: 2026-03-05
tags: []
related:
- implements: docs/stories/STORY-009-flat-navigation-model.md
- implements: docs/stories/STORY-010-read-only-relations-tab.md
- implements: docs/stories/STORY-011-simplified-border-highlighting.md
---





## Task Breakdown

### Task 1: Remove Panel enum and selected_relation from App

Remove `Panel` enum, `active_panel` field, `selected_relation` field, `move_relation_up`, `move_relation_down`, and `navigate_to_relation` methods from `app.rs`.

Replace `move_down`/`move_up` with simplified versions that always operate on `selected_doc`. Add `move_type_next`/`move_type_prev` methods that cycle `selected_type` and reset `selected_doc` to 0.

**Files:** `src/tui/app.rs`

**ACs:** STORY-009 AC1-AC5, AC7-AC8, STORY-010 AC2, AC4

### Task 2: Update key handler

Remap `h/l` to call `move_type_prev`/`move_type_next`. Remove the relation navigation branch from `j/k`. Change `Enter` to always call `enter_fullscreen`. Change `d` guard from `active_panel == Panel::DocList` to `selected_doc_meta().is_some()`.

**Files:** `src/tui/mod.rs`

**ACs:** STORY-009 AC1-AC8, STORY-010 AC2

### Task 3: Update rendering

Remove conditional border logic from `draw_type_panel` (always plain/dark gray) and `draw_doc_list` (always double/cyan). Remove `Panel` import. Strip selection indicators from `draw_relations_content`. Update help overlay text.

**Files:** `src/tui/ui.rs`

**ACs:** STORY-010 AC1, AC3, STORY-011 AC1-AC3

## Changes

- Removed `Panel` enum and `active_panel` field from App
- Simplified `move_down`/`move_up`/`move_to_top`/`move_to_bottom` to always operate on `selected_doc`
- Added `move_type_next`/`move_type_prev` methods for `h/l` type cycling
- Remapped `h/l` to `move_type_prev`/`move_type_next` in key handler
- `j/k` navigate docs by default, or relations when Relations tab is active
- `Enter` opens fullscreen by default, or navigates to selected relation when Relations tab is active
- `d` guards on `selected_doc_meta().is_some()` instead of panel check
- Types panel: static plain border, dark gray
- Doc list: cyan/double when Preview tab active, dims to plain/gray when Relations tab active
- Doc list items dim to dark gray when Relations tab is active (filenames, status, tags)
- Relations tab: cyan `>` indicator and bold cyan title on selected relation (no REVERSED)
- Relations panel: cyan border when active, gray when inactive
- Help overlay: "Switch panels" changed to "Switch type"

## Test Plan

`tests/tui_navigation_test.rs` (8 tests):
- `test_move_type_next` -- type increments, doc resets
- `test_move_type_next_resets_selected_doc` -- doc resets with docs present
- `test_move_type_prev` -- type decrements, doc resets
- `test_move_type_prev_resets_selected_doc` -- doc resets with docs present
- `test_move_type_next_clamps_at_end` -- no wrap at last type
- `test_move_type_prev_clamps_at_start` -- no wrap at first type
- `test_move_down_always_navigates_docs` -- doc increments
- `test_move_up_always_navigates_docs` -- doc decrements

## Notes

Tasks were ordered so that Task 1 changed the state model, Task 2 wired the new model to key events, and Task 3 updated the visual layer.
