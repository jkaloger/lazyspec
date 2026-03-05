---
title: "Filters Mode"
type: story
status: draft
author: "jkaloger"
date: 2026-03-05
tags: [tui]
related:
  - implements: docs/rfcs/RFC-006-tui-progressive-disclosure.md
---

## Context

As document counts grow, browsing the full list per type becomes unwieldy. The store already supports filtering by status and tag, but the TUI doesn't expose this. Filters mode replaces the type panel with filter controls and applies them to the document list in real time.

## Acceptance Criteria

### AC1: Filter controls panel

- **Given** the TUI is in Filters mode
  **When** the screen renders
  **Then** the left panel shows filter fields for status, tag, author, and sort order

### AC2: Navigate filter fields

- **Given** the TUI is in Filters mode
  **When** the user presses `j/k`
  **Then** focus moves between filter fields

### AC3: Cycle filter values

- **Given** a filter field is focused
  **When** the user presses `h/l`
  **Then** the field cycles through available values (including "all" as the default)

### AC4: Document list filters in real time

- **Given** one or more filters are active
  **When** the filter value changes
  **Then** the document list on the right updates immediately to show only matching documents

### AC5: Filtered count display

- **Given** filters are active
  **When** the document list renders
  **Then** the title shows the filtered count and total (e.g. "Documents (3 of 12)")

### AC6: Clear filters

- **Given** one or more filters are active
  **When** the user selects "clear filters" and presses Enter
  **Then** all filters reset to their defaults and the document list shows all documents

### AC7: Filters persist across mode switches

- **Given** filters are active in Filters mode
  **When** the user switches to Types mode
  **Then** the filters remain applied to the document list

### AC8: Preview and relations work with filtered list

- **Given** filters are active and a filtered document is selected
  **When** the user views the Preview or Relations tab
  **Then** the preview shows the selected filtered document's content

## Scope

### In Scope

- Filter state fields on App (status, tag, author, sort)
- Filter controls panel rendering and interaction
- Filter application to the document list
- Filter persistence across mode switches
- Sort by name, date, or status

### Out of Scope

- Free-text filter input (values are cycled from available options)
- Saving filter presets
- Type filtering (the Types mode type selector serves this purpose)
