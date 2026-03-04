---
title: Delete Confirmation Dialog
type: iteration
status: accepted
author: agent
date: 2026-03-05
tags:
- tui
- deletion
- modal
related:
- implements: docs/stories/STORY-008-delete-confirmation-dialog.md
---


## Task Breakdown

### Task 1: DeleteConfirm state and App methods

Add `DeleteConfirm` struct to `src/tui/app.rs` with `active`, `doc_path`, `doc_title`, and `references` fields. Add methods to `App`:

- `open_delete_confirm()` -- populates state from selected doc, looks up reverse references via `store.related_to()`, guards against empty doc list
- `close_delete_confirm()` -- resets state
- `confirm_delete(root)` -- calls `cli::delete::run()`, removes from store, adjusts `selected_doc` index

**ACs covered:** AC1, AC4, AC5, AC6, AC7

**Files:** `src/tui/app.rs`

### Task 2: Tests for delete dialog state

Write tests in a new `tests/tui_delete_dialog_test.rs`:

- Open dialog populates title and path from selected doc (AC1)
- Open dialog collects referencing documents (AC2)
- Open dialog with no references has empty references vec (AC3)
- Confirm delete removes file from disk and store (AC4)
- Cancel resets state without modifying files (AC5)
- Selection adjusts after deleting last item in list (AC6)
- Open on empty list does nothing (AC7)

**ACs covered:** AC1-AC7

**Files:** `tests/tui_delete_dialog_test.rs`

### Task 3: Event loop wiring

Add `delete_confirm.active` branch to the event loop in `src/tui/mod.rs`:
- `Enter` calls `app.confirm_delete(root)`
- `Esc` calls `app.close_delete_confirm()`
- All other keys are ignored (AC8)
- `d` in normal mode (DocList panel, with a selected doc) calls `app.open_delete_confirm()`

**ACs covered:** AC1, AC4, AC5, AC8

**Files:** `src/tui/mod.rs`

### Task 4: Dialog rendering

Add a `draw_delete_confirm` function to `src/tui/ui.rs` that renders the centered modal overlay with title, references list (when present), and key hints. Call it from `draw()` when `app.delete_confirm.active` is true.

**ACs covered:** AC1, AC2, AC3

**Files:** `src/tui/ui.rs`

## Changes

- Added `referenced_by()` method to `Store` for reverse-only link lookup (`src/engine/store.rs`)
- Added `DeleteConfirm` struct and `delete_confirm` field to `App` with `open_delete_confirm()`, `close_delete_confirm()`, `confirm_delete()` methods (`src/tui/app.rs`)
- Added `delete_confirm.active` branch to event loop with Enter/Esc handling and `d` keybinding guarded to DocList panel (`src/tui/mod.rs`)
- Added `draw_delete_confirm()` centered modal overlay with red border, doc title, conditional references section, key hints (`src/tui/ui.rs`)
- Added `d` to help overlay keybinding list (`src/tui/ui.rs`)
- Added 7 tests covering AC1-AC7 (`tests/tui_delete_dialog_test.rs`)

## Test Plan

| Test | AC | Assertion |
|------|----|-----------|
| `test_open_delete_populates_from_selected_doc` | AC1 | dialog active, title and path match selected doc |
| `test_open_delete_collects_references` | AC2 | references vec contains docs that link to target |
| `test_open_delete_no_references` | AC3 | references vec is empty when nothing links to target |
| `test_confirm_delete_removes_file` | AC4 | file gone from disk, doc removed from store |
| `test_cancel_delete_preserves_file` | AC5 | dialog inactive, file still on disk |
| `test_selection_adjusts_after_delete_last` | AC6 | selected_doc decrements when last item deleted |
| `test_open_delete_empty_list_noop` | AC7 | dialog stays inactive when no docs exist |

## Notes

- Resolved the `related_to()` issue by adding a dedicated `referenced_by()` method that returns only reverse links. Cleaner than filtering the mixed results.
- `confirm_delete()` calls `store.remove_file()` for immediate store consistency rather than waiting for the file watcher.
