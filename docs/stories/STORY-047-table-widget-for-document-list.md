---
title: Table widget for document list
type: story
status: accepted
author: jkaloger
date: 2026-03-08
tags: []
related:
- implements: RFC-018
---




## Context

The TUI document list currently uses a ratatui `List` widget with hand-rolled
column spacing via format strings (e.g. `{:<12}`, `{:<20}`). Columns don't
align when content varies in length, and the rendering logic is duplicated
between Types mode and Filters mode. Ratatui ships a `Table` widget that
handles column alignment natively and would eliminate these issues.

This story covers replacing `List` with `Table` in `draw_doc_list`, defining a
proper column layout, and sharing that layout across both navigation modes.

## Acceptance Criteria

- **Given** the TUI is showing the document list in Types mode
  **When** documents have varying ID lengths, title lengths, and tag counts
  **Then** all columns (tree, ID, title, status, tags) align consistently across rows

- **Given** the TUI is showing the document list in Filters mode
  **When** the same documents are displayed
  **Then** the table layout is identical to Types mode (shared rendering path)

- **Given** a parent document with children in the tree
  **When** the parent is collapsed or expanded
  **Then** the tree column shows the correct expand/collapse indicator (`▶`/`▼`) and child connectors (`├─`/`└─`)

- **Given** a virtual document appears in the list
  **When** it is rendered in the table
  **Then** `(virtual)` is appended to its title text

- **Given** the Relations tab is focused
  **When** the document list is rendered
  **Then** all table rows use dim (`DarkGray`) styling

- **Given** a row is selected in the document list
  **When** the Relations tab is not focused
  **Then** the selected row uses `Modifier::REVERSED` highlight style

- **Given** the column layout is defined
  **When** the table renders
  **Then** column widths are: tree 4 chars fixed, ID 14 chars fixed, title flexible fill, status 12 chars fixed, tags 20 chars minimum

- **Given** a document has more than 3 tags
  **When** it is rendered in the tags column
  **Then** the first 3 tags show as `[tag]` and remaining are shown as `+N`

## Scope

### In Scope

- Replace `List` widget with ratatui `Table` widget in `draw_doc_list`
- Define column layout: tree (4 fixed), ID (14 fixed), title (fill), status (12 fixed), tags (20 min)
- Share the same table rendering between Types mode and Filters mode
- Preserve tree hierarchy indicators (expand/collapse, parent/child connectors)
- Preserve `(virtual)` label on virtual documents
- Preserve dim styling when Relations tab is focused
- Preserve `Modifier::REVERSED` highlight on selected row

### Out of Scope

- Scroll behavior changes or scroll padding
- Scrollbar widget
- Tag editing or tag picker
- Status picker or status editing
- Keyboard shortcut changes
