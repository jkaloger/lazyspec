---
title: Status Picker
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

Changing a document's status currently requires switching to a terminal and running `lazyspec update <path> --status <value>`. This breaks flow when reviewing documents in the TUI. An inline status picker lets users change status without leaving the dashboard.

## Acceptance Criteria

### AC1: Open status picker

- **Given** a document is selected in the document list
  **When** the user presses `s`
  **Then** a small overlay appears listing all statuses, with the document's current status pre-selected

### AC2: Navigate statuses

- **Given** the status picker is open
  **When** the user presses `j/k`
  **Then** the selection moves between statuses

### AC3: Status colours in picker

- **Given** the status picker is open
  **When** the picker renders
  **Then** each status is displayed in its status colour (yellow for draft, blue for review, green for accepted, red for rejected, gray for superseded)

### AC4: Confirm status change

- **Given** a status is selected in the picker
  **When** the user presses Enter
  **Then** the document's frontmatter is updated on disk, the store reloads the document, the picker closes, and the document list reflects the new status

### AC5: Cancel status picker

- **Given** the status picker is open
  **When** the user presses Esc
  **Then** the picker closes with no changes made

### AC6: Picker available in Types and Filters modes

- **Given** the TUI is in Types mode or Filters mode
  **When** a document is selected and the user presses `s`
  **Then** the status picker opens

## Scope

### In Scope

- Status picker overlay rendering (positioned near the selected document)
- `j/k`/Enter/Esc interaction within the picker
- Frontmatter write-back for status changes
- Store reload after status change

### Out of Scope

- Batch status changes (selecting multiple documents)
- Status change history or audit log
- Status transition rules (any status can change to any other)
