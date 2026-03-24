---
title: "TUI Filters View"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: []
related: []
---

## Acceptance Criteria

### AC: filter-field-cycling

Given the TUI is in Filters mode with Status focused
When the user presses Tab
Then focus moves to Tag, then ClearAction, then back to Status

### AC: status-filter-cycling

Given the Status field is focused
When the user presses l (or Right) repeatedly
Then the value cycles through all, draft, review, accepted, rejected, superseded, then back to all

### AC: tag-filter-cycling

Given the Tag field is focused and documents have tags "api" and "tui"
When the user presses l (or Right) repeatedly
Then the value cycles through all, api, tui, then back to all

### AC: filter-application

Given a status filter of "draft" is active
When the document list renders
Then only documents with status draft appear in the filtered list

### AC: combined-filters

Given a status filter of "accepted" and a tag filter of "tui" are both active
When the document list renders
Then only documents matching both filters appear

### AC: filtered-count-display

Given filters are active and 3 of 12 documents match
When the document list renders
Then the table title reads "Documents (3 of 12)"

### AC: clear-filters

Given one or more filters are active and ClearAction is focused
When the user presses Enter
Then both filter fields reset to all, focus returns to Status, and the full document list is restored

### AC: cache-invalidation

Given a filtered list is displayed
When the user cycles a filter value
Then the cached result is discarded and the list recomputes on the next render

### AC: layout-structure

Given the TUI is in Filters mode
When the screen renders
Then the left 20% shows the Filters panel and the right 80% is split into a document table (top 40%) and preview pane (bottom 60%)

### AC: mode-exit-resets-filters

Given the TUI is in Filters mode with an active status filter
When the user presses backtick to leave Filters mode
Then the active filters are reset before the mode transitions
