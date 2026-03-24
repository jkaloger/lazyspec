---
title: "TUI Search Overlay"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: []
related: []
---

## Acceptance Criteria

### AC: search-activation

- **Given** the TUI is in normal view mode or Filters view mode
  **When** the user presses `/`
  **Then** search mode activates, the query is empty, and the search overlay renders

### AC: search-mode-consumes-input

- **Given** search mode is active
  **When** the user presses any character key
  **Then** the character appends to the search query and no other handler receives the event

### AC: query-updates-results

- **Given** search mode is active
  **When** the user types a query that matches one or more document titles, tags, or paths
  **Then** matching documents appear in the results list, sorted alphabetically by path

### AC: empty-query-clears-results

- **Given** search mode is active with a non-empty query
  **When** the user deletes all characters with Backspace
  **Then** the results list is empty

### AC: result-navigation-down

- **Given** search mode is active with multiple results and the first result selected
  **When** the user presses Ctrl-j or Down arrow
  **Then** the selection moves to the next result

### AC: result-navigation-up

- **Given** search mode is active with multiple results and a non-first result selected
  **When** the user presses Ctrl-k or Up arrow
  **Then** the selection moves to the previous result

### AC: selection-navigates-to-document

- **Given** search mode is active with results and a result selected
  **When** the user presses Enter
  **Then** the TUI navigates to the selected document's type tab and highlights it in the doc list, and search mode exits

### AC: escape-exits-search

- **Given** search mode is active
  **When** the user presses Escape
  **Then** search mode exits without navigating, and the query and results are cleared

### AC: git-status-gutters-in-results

- **Given** search mode is active with results that include git-tracked changes
  **When** the results list renders
  **Then** new files show a green gutter, modified files show a yellow gutter, and unchanged files show no gutter
