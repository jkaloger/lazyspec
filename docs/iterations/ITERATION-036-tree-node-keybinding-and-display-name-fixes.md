---
title: Tree node keybinding and display name fixes
type: iteration
status: accepted
author: agent
date: 2026-03-08
tags: []
related:
- implements: STORY-042
---




## Changes

### Task 1: Change expand/collapse keybinding to spacebar

**ACs addressed:** AC2, AC3

**Files:**
- Modify: `src/tui/app.rs`
- Modify: `src/tui/ui.rs`

**What to implement:**

Replace the `h`/Left and `l`/Right expand/collapse keybindings with spacebar. In the key event handler for ViewMode::Types:

- Remove the expand/collapse branches from `h`/Left and `l`/Right so they revert to their original type-switching behavior.
- Add a spacebar (`KeyCode::Char(' ')`) handler: if the selected node is a collapsed parent, expand it. If expanded parent, collapse it. If a child, jump to parent and collapse. Same logic as the current `h`/`l` handlers, just on a different key.
- Update the help overlay text to reflect the new keybinding.

### Task 2: Add id field to DocMeta and use in TUI list

**ACs addressed:** AC1

**Files:**
- Modify: `src/engine/document.rs`
- Modify: `src/engine/store.rs`
- Modify: `src/tui/app.rs`
- Modify: `src/tui/ui.rs`

**What to implement:**

Add `pub id: String` to `DocMeta`. Compute it during `Store::load()` by extracting the `TYPE-NNN` prefix from the path:
- Flat file `docs/rfcs/RFC-001-my-first-rfc.md` → `RFC-001`
- Subfolder `docs/rfcs/RFC-001-my-first-rfc/index.md` → `RFC-001`
- Virtual parent `docs/rfcs/RFC-001-my-first-rfc/.virtual` → `RFC-001`
- Child document `docs/rfcs/RFC-001/threat-model.md` → `threat-model`

The ID is the file stem (or parent folder name for index.md) up to and including the first numeric segment. Use a simple split: take everything before the second `-` after the digits. For child documents that don't match the TYPE-NNN pattern, use the file stem as-is.

Add `id: String` to `DocListNode` as well, populated from `DocMeta.id` in `build_doc_tree()`.

In `doc_list_node_spans()`, render as `{id:<12} {title:<20}` so both are visible columns. Replace the current `{title:<28}` with this two-column format.

## Test Plan

### T1: Spacebar expand/collapse (unit)

Verify that pressing spacebar on a collapsed parent expands it, and pressing again collapses it. Verify h/l still switch types.

### T2: ID extraction from paths (unit)

Verify that the ID extraction logic returns:
- `RFC-001` from `docs/rfcs/RFC-001-foo.md`
- `RFC-001` from `docs/rfcs/RFC-001-foo/index.md`
- `threat-model` from `docs/rfcs/RFC-001/threat-model.md`

## Notes

Regression in ITERATION-035: `doc_list_node_spans()` used `node.title` alone, losing the document ID prefix (e.g. RFC-001) that was previously visible via `display_name()`. The fix adds the ID as a separate column before the title.
