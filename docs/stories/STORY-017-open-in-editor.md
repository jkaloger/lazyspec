---
title: Open in Editor
type: story
status: accepted
author: jkaloger
date: 2026-03-05
tags:
- tui
related:
- implements: docs/rfcs/RFC-006-tui-progressive-disclosure.md
- implements: docs/rfcs/RFC-011-tui-ux-refinements.md
---



## Context

The TUI is read-only for document body content. Editing a document means finding its path, switching to another terminal, and opening it manually. Pressing `e` should open the selected document in the user's preferred editor, with the TUI suspending and resuming cleanly around the editor session.

## Acceptance Criteria

### AC1: Open document in $EDITOR

- **Given** a document is selected in the document list
  **When** the user presses `e`
  **Then** the TUI suspends, the document opens in `$EDITOR`, and the user can edit it

### AC2: Editor fallback chain

- **Given** `$EDITOR` is not set
  **When** the user presses `e`
  **Then** the TUI falls back to `$VISUAL`, then to `vi`

### AC3: TUI resumes after editor exits

- **Given** the editor is open with a document
  **When** the editor process exits
  **Then** the TUI resumes with the alternate screen restored and raw mode re-enabled

### AC4: Document reloads after editing

- **Given** the user edited a document's frontmatter or body in the editor
  **When** the TUI resumes
  **Then** the store reloads the edited document and the display reflects any changes

### AC5: Editor available in Types, Filters, and Graph modes

- **Given** the TUI is in Types, Filters, or Graph mode
  **When** a document is selected and the user presses `e`
  **Then** the editor opens for that document

## Scope

### In Scope

- Terminal suspend (leave alternate screen, disable raw mode)
- Spawning the editor process with the document's full path
- Terminal resume (re-enter alternate screen, enable raw mode)
- Document reload on resume
- `$EDITOR` / `$VISUAL` / `vi` fallback chain

### Out of Scope

- Inline editing within the TUI itself
- Opening multiple documents simultaneously
- Editor plugins or integrations
