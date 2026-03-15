---
title: Validation diagnostic for duplicate IDs
type: story
status: accepted
author: jkaloger
date: 2026-03-13
tags: []
related:
- implements: docs/rfcs/RFC-020-fix-command-numbering-conflict-resolution.md
---



## Context

The store loads documents keyed by full path, but `extract_id()` can produce the same ID for multiple documents (e.g. two files both resolve to `RFC-020`). Today `lazyspec validate` has no awareness of this, so duplicate IDs go undetected unless the user happens to run `fix`. Adding a duplicate-ID diagnostic to validation gives users immediate visibility into numbering conflicts.

## Acceptance Criteria

- **Given** a store with two or more documents whose `extract_id()` returns the same ID
  **When** `lazyspec validate` is run
  **Then** a `DuplicateId` issue is reported listing the conflicting ID and all document paths that share it

- **Given** a store with two or more documents sharing an ID
  **When** `lazyspec validate --json` is run
  **Then** the JSON output includes the duplicate-ID diagnostic in the `errors` array with the conflicting ID and paths

- **Given** a store with two or more documents sharing an ID
  **When** `lazyspec validate` is run without `--json`
  **Then** the human-readable output includes a line describing the duplicate ID and the conflicting paths

- **Given** a store where every document has a unique extracted ID
  **When** `lazyspec validate` is run
  **Then** no duplicate-ID diagnostics are emitted

- **Given** a document with `validate_ignore: true` that shares an ID with another document
  **When** `lazyspec validate` is run
  **Then** the ignored document is excluded from duplicate-ID grouping

## Scope

### In Scope

- New `DuplicateId` variant on `ValidationIssue` in `engine/validation.rs`
- Grouping logic in `validate_full` that collects documents by extracted ID and emits an issue for any ID with more than one document
- `Display` implementation for the new variant
- Surfacing the diagnostic in both human-readable and JSON output of `lazyspec validate`

### Out of Scope

- Automatic conflict resolution or file renumbering (Story 1 and 2)
- Changes to `show`, TUI, or `resolve_shorthand` for duplicate handling (Story 3)
- New CLI flags or configuration options for controlling duplicate-ID severity
