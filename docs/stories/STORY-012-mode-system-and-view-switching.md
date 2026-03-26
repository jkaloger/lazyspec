---
title: Mode System and View Switching
type: story
status: accepted
author: jkaloger
date: 2026-03-05
tags:
- tui
related:
- implements: RFC-006
---



## Context

The TUI currently has a single, fixed layout: type panel on the left, document list and preview on the right. New capabilities (filters, metrics, graph) each need different right-side layouts, but the rendering pipeline assumes the same three-panel structure. A mode system allows the left panel to determine what the entire screen shows, with each mode owning the full layout.

## Acceptance Criteria

### AC1: View mode enum and state

- **Given** the TUI is running
  **When** the application initialises
  **Then** the view mode defaults to Types and is displayed in the title bar

### AC2: Backtick cycles modes

- **Given** the TUI is in Types mode
  **When** the user presses backtick
  **Then** the mode advances to the next mode in the cycle (Types -> Filters -> Metrics -> Graph -> Types)

### AC3: Mode indicator

- **Given** the TUI is in any mode
  **When** the screen renders
  **Then** the title bar shows the current mode name and a hint that backtick cycles modes

### AC4: Rendering dispatch

- **Given** the TUI is in a non-Types mode
  **When** the screen renders
  **Then** the right side shows the layout appropriate to that mode (skeleton panels with titles and borders are sufficient for this story)

### AC5: Types mode unchanged

- **Given** the TUI is in Types mode
  **When** the user navigates, previews, or interacts with documents
  **Then** all existing behaviour works identically to before the mode system was added

## Scope

### In Scope

- `ViewMode` enum (Types, Filters, Metrics, Graph)
- Backtick key handler for mode cycling
- Mode indicator in the title bar
- Rendering dispatch that calls mode-specific draw functions
- Skeleton renderers for Filters, Metrics, and Graph modes (borders and titles only)

### Out of Scope

- Actual content for Filters, Metrics, or Graph modes (covered by their own stories)
- Changes to existing Types mode behaviour
