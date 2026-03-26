---
title: TUI Dashboard
type: story
status: accepted
author: jkaloger
date: 2026-03-04
tags:
- tui
- ratatui
- dashboard
related:
- implements: RFC-001
---


## Context

lazyspec needs a terminal UI for browsing and previewing documents. The TUI launches when `lazyspec` is run with no subcommand. It provides a three-panel dashboard with vim-style navigation, rendered markdown previews, fuzzy search, and live file watching.

## Acceptance Criteria

### AC1: Three-panel layout

**Given** the TUI is launched
**When** the app renders
**Then** a three-panel layout is displayed: type selector (left), document list (top right), markdown preview (bottom right)

### AC2: Type selector

**Given** the left panel is focused
**When** the user navigates with j/k
**Then** the selected document type changes and the document list updates to show documents of that type with counts

### AC3: Document list

**Given** the document list panel is focused
**When** the user navigates with j/k
**Then** the selected document changes and the preview panel updates with the rendered markdown content

### AC4: Markdown preview

**Given** a document is selected in the list
**When** the preview panel renders
**Then** the document's markdown body is rendered with formatting (headers, lists, code blocks) using tui-markdown

### AC5: Full-screen document view

**Given** a document is selected
**When** the user presses Enter
**Then** a full-screen scrollable view of the document is shown, dismissible with Esc

### AC6: Fuzzy search

**Given** the TUI is active
**When** the user presses `/` and types a query
**Then** documents are filtered in real-time across all types by fuzzy matching on title and tags using nucleo

### AC7: Help overlay

**Given** the TUI is active
**When** the user presses `?`
**Then** a help overlay showing all keybindings is displayed, dismissible with Esc or `?`

### AC8: File watching

**Given** the TUI is running
**When** a document file is modified on disk (by another process or editor)
**Then** the store re-parses the affected file and the display updates automatically

### AC9: Status colors

**Given** documents with different statuses
**When** they are rendered in the document list
**Then** statuses are color-coded: draft (yellow), review (blue), accepted (green), rejected (red), superseded (grey)

### AC10: Navigation

**Given** the TUI is active
**When** the user presses h/l
**Then** focus moves between the type selector and document list panels

## Scope

### In Scope

- Ratatui + crossterm rendering
- Three-panel layout with focus management
- Vim-style keybindings (h/j/k/l/g/G/Enter/Esc/q)
- Fuzzy search with nucleo
- Markdown rendering with tui-markdown
- File watching with notify
- Status color coding
- Help overlay

### Out of Scope

- Document editing within the TUI
- Creating or deleting documents from the TUI
- CLI commands
