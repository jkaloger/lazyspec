---
title: Read-Only Relations Tab
type: story
status: accepted
author: jkaloger
date: 2026-03-05
tags: []
related:
- implements: docs/rfcs/RFC-005-tui-flat-navigation-model.md
---



## Context

The Relations tab currently has its own selection state (`selected_relation`) and hijacks `j/k` when active. It shows a `> ` indicator and reversed highlighting on the selected relation. This adds navigational complexity that doesn't justify itself. The relations tab should be a passive information display.

## Acceptance Criteria

### AC1: No selection indicator in relations

- **Given** a document with relations is selected and the Relations tab is active
  **When** the relations list renders
  **Then** no item shows a `> ` prefix or reversed/highlighted style

### AC2: j/k do not navigate relations

- **Given** the Relations tab is active
  **When** the user presses `j` or `k`
  **Then** the document list selection changes (not the relations list)

### AC3: Relations still display

- **Given** a document with relations is selected
  **When** the user switches to the Relations tab with `Tab`
  **Then** all relations are listed grouped by type, with title and status

### AC4: selected_relation state is removed

- **Given** the application state
  **When** inspecting the App struct
  **Then** there is no `selected_relation` field

## Scope

### In Scope

- Removing `selected_relation` from App state
- Removing `move_relation_up`, `move_relation_down`, `navigate_to_relation` methods
- Removing selection indicators from relations rendering
- Removing the `j/k` relation navigation branch from key handling

### Out of Scope

- Changing the Relations tab layout or grouped display
- Navigation model changes (STORY-009)
- Border changes (STORY-011)
