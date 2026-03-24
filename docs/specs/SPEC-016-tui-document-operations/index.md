---
title: "TUI Document Operations"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: [tui, overlays, forms]
related:
  - related-to: docs/architecture/ARCH-005-tui/app-state.md
  - related-to: docs/stories/STORY-006-create-form-ui-and-input-handling.md
  - related-to: docs/stories/STORY-007-document-creation-on-submit.md
  - related-to: docs/stories/STORY-008-delete-confirmation-dialog.md
  - related-to: docs/stories/STORY-016-status-picker.md
  - related-to: docs/stories/STORY-046-tui-warnings-panel.md
  - related-to: docs/stories/STORY-073-tui-async-document-creation.md
---

## Summary

The TUI provides six modal overlay dialogs for document lifecycle operations: creating documents, deleting documents, changing status, adding relations, reviewing warnings, and triggering the fix command. Each overlay is an independent state machine stored as a struct on `App`, activated by a keybinding and dismissed with Escape or a confirm action. All overlays render as centered popups using ratatui's `Clear` widget to punch through the underlying dashboard.

## Create Form

@ref src/tui/state/forms.rs#CreateForm

The create form opens when the user presses `n` from normal mode. `open_create_form` resets the form state and sets `doc_type` to the currently selected type in the type panel.

@ref src/tui/state/app.rs#open_create_form

The form presents four fields: Title, Author, Tags, and Related. Field focus cycles with Tab (forward) and Shift+Tab (backward) through the `FormField` enum, which defines the order Title, Author, Tags, Related. Each field is a plain `String`; text input appends characters and backspace pops. The focused field's label renders in cyan with a trailing underscore cursor.

@ref src/tui/state/forms.rs#FormField

On Enter, `submit_create_form` validates that the title is non-empty and that any relation string resolves to an existing document. Tags are parsed as a comma-separated string. Relations accept the format `type:ID` (e.g. `implements:RFC-001`) or a bare ID that defaults to `related-to`. If the document type uses `NumberingStrategy::Reserved`, the submission spawns a background thread that performs the git remote operations and sends `AppEvent::CreateProgress` messages back through the channel. The form enters a loading state where inputs are disabled and a status message (e.g. "Querying remote for latest tag...") replaces the footer. Non-reserved types submit synchronously on the main thread.

@ref src/tui/state/app.rs#submit_create_form

On success, the form closes and the TUI navigates to the new document by selecting its type and scrolling to its position in the doc tree. On failure, the form re-enables inputs and displays the error inline.

## Delete Confirmation

@ref src/tui/state/forms.rs#DeleteConfirm

Pressing `d` with a document selected calls `open_delete_confirm`, which pre-fetches all documents that reference the target via `store.referenced_by()`. The dialog displays the document title and, if references exist, lists each referencing document with its relation type. If no references exist, the references section is omitted entirely. The popup height adjusts dynamically based on reference count.

@ref src/tui/state/app.rs#confirm_delete

Enter delegates to `cli::delete::run()`, removes the document from the store, rebuilds the doc tree, and clamps the selection index to stay in bounds. Escape cancels without modification.

## Status Picker

@ref src/tui/state/forms.rs#StatusPicker

The status picker opens with `s` and lists all five statuses: Draft, Review, Accepted, Rejected, Superseded. The current document's status is pre-selected by index. Each status renders in its assigned color (yellow for Draft, blue for Review, green for Accepted, red for Rejected, gray for Superseded). Navigation uses `j`/`k`. Enter writes the new status to the document's frontmatter via `cli::update::run()`, reloads the store, and closes the picker.

@ref src/tui/state/app.rs#confirm_status_change

The picker is available in both Types and Filters view modes. If no document is selected, the keybinding is a no-op.

## Link Editor

@ref src/tui/state/forms.rs#LinkEditor

The link editor opens with `r` and provides two controls: a relation type selector and a search field. The relation type cycles through `implements`, `supersedes`, `blocks`, and `related-to` (the `REL_TYPES` constant sourced from `RelationType::ALL_STRS`) via Tab. The search field filters all documents in the store by matching the lowercased query against each document's `ID: Title` string. Results update live on each keystroke via `update_link_search`, which excludes the source document and sorts alphabetically. The selected result is highlighted with `>` prefix and cyan bold styling. If no documents match, "(no matches)" is displayed.

@ref src/tui/state/app.rs#confirm_link

Enter calls `cli::link::link()` to write the relation into the source document's frontmatter, reloads the store, and closes the editor.

## Warnings Panel

The warnings panel toggles with `w` and aggregates three sources: parse errors from the store, validation errors, and validation warnings. The `total_warnings_count` method sums all three. Each entry renders as two lines: the file path or error message on the first line, and a description on the second. Parse errors show in yellow, validation errors in red, validation warnings in yellow. The list is scrollable with `j`/`k`.

When no warnings exist, the panel shows either "No warnings" or the output of the most recent fix run.

## Fix Integration

Pressing `f` inside the warnings panel sets `fix_request = true`. The event loop picks this up on the next tick, runs `cli::fix::run_human()` against all parse error paths, reloads the store, refreshes validation, and stores the fix output in `fix_result` for display. This is a synchronous operation that blocks the event loop for the duration of the fix.
