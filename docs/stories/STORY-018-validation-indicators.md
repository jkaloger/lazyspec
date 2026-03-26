---
title: Validation Indicators
type: story
status: draft
author: jkaloger
date: 2026-03-05
tags:
- tui
related:
- implements: RFC-006
---


## Context

`Store::validate()` detects broken links, unlinked iterations, and unlinked ADRs, but this information is only available through the CLI's `lazyspec validate` command. Users browsing in the TUI have no indication that a document has issues until they explicitly run validation in a separate terminal.

## Acceptance Criteria

### AC1: Validation runs on store load

- **Given** the TUI starts and loads the store
  **When** the initial render occurs
  **Then** validation has run and documents with errors are identified

### AC2: Error indicator in document list

- **Given** a document has one or more validation errors
  **When** the document list renders (in any mode that shows documents)
  **Then** the document row shows a `!` prefix before the filename

### AC3: No indicator for valid documents

- **Given** a document has no validation errors
  **When** the document list renders
  **Then** no `!` prefix appears

### AC4: Validation refreshes on document changes

- **Given** a document is created, deleted, or its status changes via the TUI
  **When** the store reloads the affected document
  **Then** validation results are recalculated and indicators update accordingly

### AC5: Validation errors visible in preview

- **Given** a document with validation errors is selected
  **When** the preview panel renders
  **Then** the validation errors for that document are shown (e.g. "broken link: target.md", "iteration without story link")

## Scope

### In Scope

- Running `Store::validate()` on startup and caching results as `HashSet<PathBuf>`
- `!` prefix rendering in the document list
- Refreshing validation on document create/delete/update
- Showing per-document errors in the preview panel

### Out of Scope

- Auto-fixing validation errors
- Validation rules beyond what `Store::validate()` already checks
- Blocking document creation based on validation
