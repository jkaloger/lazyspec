---
title: "TUI Document Operations"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: [tui, overlays, forms]
related:
  - related-to: docs/stories/STORY-006-create-form-ui-and-input-handling.md
  - related-to: docs/stories/STORY-007-document-creation-on-submit.md
  - related-to: docs/stories/STORY-008-delete-confirmation-dialog.md
  - related-to: docs/stories/STORY-016-status-picker.md
  - related-to: docs/stories/STORY-046-tui-warnings-panel.md
  - related-to: docs/stories/STORY-073-tui-async-document-creation.md
---

## Acceptance Criteria

### AC: create-form-opens-with-selected-type

Given the user is in normal mode with a type selected in the type panel
When the user presses `n`
Then a centered create form overlay appears with the title "Create {Type}" matching the selected type, and focus is on the Title field

### AC: create-form-field-navigation

Given the create form is open
When the user presses Tab
Then focus advances through Title, Author, Tags, Related and wraps back to Title; Shift+Tab reverses the order

### AC: create-form-title-validation

Given the create form is open with an empty Title field
When the user presses Enter
Then an error message "Title is required" is displayed on the form and no document is created

### AC: create-form-synchronous-submit

Given the create form is filled with a valid title for a non-reserved numbering type
When the user presses Enter
Then a document is created on disk, the store reloads, the form closes, and the TUI navigates to the new document in the doc tree

### AC: create-form-async-reserved-numbering

Given the create form is filled for a document type with `numbering = "reserved"`
When the user presses Enter
Then a background thread is spawned, the form enters a loading state with disabled inputs and a "Reserving..." status message, and progress messages update as the reservation proceeds

### AC: create-form-async-error-recovery

Given a background reservation thread completes with an error
When the `CreateComplete` event arrives with an `Err`
Then the form re-enables its inputs and displays the error message, allowing the user to retry or press Escape to cancel

### AC: delete-confirm-shows-references

Given a document is selected that other documents reference
When the user presses `d`
Then a delete confirmation dialog appears listing the document title and all referencing documents with their relation types

### AC: delete-confirm-no-references

Given a document is selected that no other documents reference
When the user presses `d`
Then the delete confirmation dialog appears with only the document title and no references section

### AC: delete-removes-and-clamps

Given the delete confirmation dialog is open
When the user presses Enter
Then the document is removed from disk via `cli::delete::run()`, the store updates, the doc tree rebuilds, and the selection index clamps to stay within bounds

### AC: status-picker-preselects-current

Given a document with status "Review" is selected
When the user presses `s`
Then the status picker opens with "Review" pre-highlighted at index 1, and each status renders in its assigned color

### AC: status-picker-writes-frontmatter

Given the status picker is open with a status selected
When the user presses Enter
Then the document's frontmatter status field is updated on disk, the store reloads, and the picker closes

### AC: link-editor-live-search

Given the link editor is open
When the user types characters into the search field
Then the results list filters to documents whose "ID: Title" contains the query, excluding the source document, and updates on each keystroke

### AC: link-editor-relation-type-cycling

Given the link editor is open
When the user presses Tab
Then the relation type cycles through implements, supersedes, blocks, and related-to

### AC: warnings-panel-aggregates-sources

Given the store has parse errors and validation has produced errors and warnings
When the user presses `w`
Then the warnings panel opens showing all three categories: parse errors (yellow path + message), validation errors (red), and validation warnings (yellow), scrollable with j/k

### AC: fix-from-warnings-panel

Given the warnings panel is open with parse errors listed
When the user presses `f`
Then `cli::fix::run_human()` executes against all parse error paths, the store reloads, validation refreshes, and the fix output replaces the panel content
