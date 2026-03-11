---
title: TUI Document Deletion
type: rfc
status: accepted
author: jkaloger
date: 2026-03-05
tags:
  - tui
  - deletion
  - modal
related:
  - related-to: docs/rfcs/RFC-001-my-first-rfc.md
  - related-to: docs/rfcs/RFC-003-tui-document-creation.md
---

## Summary

Add document deletion to the TUI via a confirmation dialog. Users press `d` on a selected document to open a modal that shows the document title and any documents referencing it. Enter confirms, Esc cancels.

Currently the TUI is read-only for destructive operations. Deletion requires dropping to the CLI (`lazyspec delete <path>`), breaking the browsing flow. STORY-003 explicitly scoped this out; this RFC brings it in.

## Design Intent

Deletion is destructive and irreversible (outside of git). The dialog serves two purposes: prevent accidental deletion and surface the impact of removing a document that other documents reference. The user always makes the final call.

The implementation follows the modal overlay pattern from RFC-003. A `DeleteConfirm` struct captures the pending deletion state, checked in the event loop the same way `create_form.active` and `search_mode` are checked today. The actual file removal delegates to `cli::delete::run()`, and the file watcher handles store refresh.

### Dialog Layout

```
┌─ Delete? ─────────────────────────────────────┐
│                                                │
│  Delete "My Document"?                         │
│                                                │
│  Referenced by:                                │
│    • STORY-002 (implements)                    │
│    • ITERATION-001 (related-to)                │
│                                                │
│           [Enter: delete]  [Esc: cancel]       │
└────────────────────────────────────────────────┘
```

When no documents reference the target, the "Referenced by" section is omitted.

### Interaction

- `d` from normal mode (with a document selected in the DocList panel) opens the dialog
- `Enter` confirms and deletes the file
- `Esc` cancels and returns to normal mode
- No other keys are handled while the dialog is open

### State Model

```
@draft DeleteConfirm {
    active: bool,
    doc_path: PathBuf,       // path of the document to delete
    doc_title: String,       // title for display
    references: Vec<(String, PathBuf)>,  // (relation_type, referencing_doc_path)
}
```

`src/tui/app.rs#App` gains a `delete_confirm: DeleteConfirm` field. The event loop checks `app.delete_confirm.active` as a mode, slotting in alongside `create_form.active`, `search_mode`, and `fullscreen_doc`.

### Deletion Flow

1. User presses `d` with a document selected
2. `App::open_delete_confirm()` populates `DeleteConfirm` with the selected document's path, title, and referencing documents (via `store.related_to()`)
3. Dialog renders over the main layout
4. On `Enter`: call `cli::delete::run(root, &doc_path)` to remove the file
5. The file watcher detects the removal and reloads the store
6. Adjust `selected_doc` index to stay in bounds (clamp to `len - 1`, or 0 if the list is now empty)
7. Close the dialog

On `Esc`, close the dialog with no side effects.

### Edge Cases

- If the document list is empty (no document selected), `d` does nothing
- After deleting the last document of a type, `selected_doc` resets to 0
- The dialog is only reachable from the DocList panel, not the Types panel

### Config Dependency

Same as RFC-003. The `root` path is available via `app.store.root()` and is passed through on confirmation. No additional config needed.

## Stories

1. **Delete confirmation dialog** -- `DeleteConfirm` state, modal rendering, `d` keybinding, reference lookup, `Enter`/`Esc` handling, delegation to `cli::delete::run()`, and selection adjustment after deletion.
