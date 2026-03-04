---
title: Delete Confirmation Dialog
type: story
status: accepted
author: jkaloger
date: 2026-03-05
tags:
- tui
- deletion
- modal
related:
- implements: docs/rfcs/RFC-004-tui-document-deletion.md
---


## Context

The TUI currently supports document creation but not deletion. Users must drop to the CLI to remove documents, breaking their browsing flow. This story adds a confirmation dialog that lets users delete a selected document without leaving the TUI, while surfacing any documents that reference the target so users understand the impact.

## Acceptance Criteria

### AC1: Open delete dialog

**Given** a document is selected in the DocList panel
**When** the user presses `d`
**Then** a centered confirmation dialog appears showing the document's title

### AC2: Show referencing documents

**Given** the delete dialog is open for a document that other documents reference
**When** the dialog renders
**Then** the referencing documents and their relation types are listed in the dialog

### AC3: No references

**Given** the delete dialog is open for a document with no incoming references
**When** the dialog renders
**Then** the references section is omitted

### AC4: Confirm deletion

**Given** the delete dialog is open
**When** the user presses `Enter`
**Then** the document file is removed from disk and the store updates to reflect the removal

### AC5: Cancel deletion

**Given** the delete dialog is open
**When** the user presses `Esc`
**Then** the dialog closes and no file is modified

### AC6: Selection adjustment

**Given** a document has just been deleted
**When** the document list re-renders
**Then** the selection index stays in bounds (moves up if the deleted item was last in the list)

### AC7: No document selected

**Given** the DocList panel is focused but empty (no documents of this type exist)
**When** the user presses `d`
**Then** nothing happens

### AC8: Dialog blocks other input

**Given** the delete dialog is open
**When** the user presses any key other than `Enter` or `Esc`
**Then** the keypress is ignored

## Scope

### In Scope

- Delete confirmation modal overlay
- Keybinding (`d`) from the DocList panel
- Reference lookup and display
- Delegation to existing `cli::delete::run()`
- Selection index adjustment after deletion

### Out of Scope

- Bulk/multi-document deletion
- Undo/trash (deletion is permanent, recoverable via git only)
- Cleaning up stale references in other documents after deletion
