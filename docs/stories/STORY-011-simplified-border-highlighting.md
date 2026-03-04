---
title: Simplified Border Highlighting
type: story
status: accepted
author: jkaloger
date: 2026-03-05
tags: []
related:
- implements: docs/rfcs/RFC-005-tui-flat-navigation-model.md
---



## Context

With the panel focus model removed, the conditional border styling (cyan/double for active panel, dark gray/plain for inactive) no longer makes sense. The Types panel is never "active" in the old sense, and the doc list is always the navigable surface. Borders should reflect this static reality. The help overlay also references the old "Switch panels" keybinding text.

## Acceptance Criteria

### AC1: Types panel has a static border

- **Given** the dashboard is displayed
  **When** the Types panel renders
  **Then** it always uses a plain border with dark gray colour, regardless of any selection state

### AC2: Document list always has an active border

- **Given** the dashboard is displayed
  **When** the document list renders
  **Then** it always uses a double border with cyan colour

### AC3: Help overlay reflects new keybindings

- **Given** the help overlay is open
  **When** the user reads the keybinding list
  **Then** `h/l` is described as "Switch type" (not "Switch panels")

## Scope

### In Scope

- Removing conditional border logic from `draw_type_panel` and `draw_doc_list`
- Types panel: always plain border, dark gray
- Doc list: always double border, cyan
- Updating help overlay text

### Out of Scope

- Changing the preview/relations tab border style
- Layout changes (panel sizes, positions)
- Navigation model changes (STORY-009)
- Relations tab changes (STORY-010)
