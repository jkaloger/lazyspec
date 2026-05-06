---
title: Expandable document list rows
type: story
status: draft
author: jkaloger
date: 2026-04-30
tags: []
related:
- implements: RFC-040
---


## Context

Implements RFC-040 section 1: document list rows default to 1 line with `e` key toggling expansion to show full content up to a configurable max height.

## Acceptance Criteria

- **AC1:** **Given** a document list with long content, **When** I press `e` on a selected row, **Then** the row expands to show full content up to max_expanded_height
- **AC2:** **Given** an expanded row, **When** I press `e` again, **Then** the row collapses back to 1 line
- **AC3:** **Given** a row with content exceeding 1 line, **When** viewing the list, **Then** a ▸ indicator is shown
- **AC4:** **Given** an expanded row, **When** viewing the list, **Then** a ▾ indicator is shown
- **AC5:** **Given** the config file with max_expanded_height set, **When** rows are expanded, **Then** they expand to that maximum height
- **AC6:** **Given** a row with content fitting in 1 line, **When** viewing the list, **Then** no expand indicator is shown

## Scope

### In Scope

- Default row height of 1 line in document list
- `e` keybinding to toggle row expansion in document list view
- Visual indicators: ▸ for collapsed rows with expandable content, ▾ for expanded rows
- Configuration: max_expanded_height (default 5), indicator_collapsed ("▸"), indicator_expanded ("▾")
- State tracking via ExpandedRows with HashSet<usize> for expanded row indices

### Out of Scope

- Expansion in views other than document list
- Mouse interaction for expanding/collapsing rows
- Persisting expanded state between sessions
- Horizontal expansion of rows
