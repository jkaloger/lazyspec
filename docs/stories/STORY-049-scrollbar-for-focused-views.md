---
title: Scrollbar for focused views
type: story
status: accepted
author: jkaloger
date: 2026-03-08
tags: []
related:
- implements: docs/rfcs/RFC-018-tui-interaction-enhancements.md
---



## Context

Scrollable views in the TUI (document list, fullscreen preview, relations list) currently provide no visual indication of scroll position or content overflow. Users have no way to gauge how much content exists beyond the visible area. This story adds a `Scrollbar` widget from ratatui to focused scrollable views, following the styling defined in RFC-018 Section 4.

## Acceptance Criteria

- **Given** the document list (Types mode or Filters mode) has more items than fit in the visible area
  **When** the document list view is focused
  **Then** a scrollbar renders on the right edge of the content area, inside the border

- **Given** the document list view is not focused (dimmed)
  **When** the list overflows the visible area
  **Then** no scrollbar is rendered

- **Given** the fullscreen preview is open with content longer than the visible area
  **When** the user scrolls through the preview
  **Then** a scrollbar renders on the right edge, and the thumb position reflects the current scroll offset

- **Given** the relations list overflows its visible area
  **When** the relations list view is focused
  **Then** a scrollbar renders on the right edge of the relations content area

- **Given** any scrollable view is focused and content does not overflow
  **When** the view renders
  **Then** no scrollbar is shown

- **Given** a scrollbar is visible
  **When** rendered
  **Then** the track uses `DarkGray` styling and the thumb uses `Cyan` to match the focused border color

- **Given** a scrollbar is visible
  **When** the user scrolls
  **Then** `ScrollbarState` reflects the correct total content length and current offset

## Scope

### In Scope

- Add ratatui `Scrollbar` widget to document list in Types and Filters modes
- Add `Scrollbar` widget to fullscreen preview
- Add `Scrollbar` widget to relations list
- Only render scrollbar when the view is focused (not dimmed) and content overflows the visible area
- Use `ScrollbarState` seeded with total content length and current scroll offset
- Style the scrollbar with a thin track in `DarkGray` and thumb in `Cyan`

### Out of Scope

- Table widget changes (covered by separate story)
- Viewport or scroll behavior changes (covered by scroll padding story)
- Tag editing functionality
- Scrollbar for any views not listed (e.g., help overlay)
