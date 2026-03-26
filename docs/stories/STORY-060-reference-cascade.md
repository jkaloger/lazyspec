---
title: Reference cascade
type: story
status: review
author: jkaloger
date: 2026-03-13
tags: []
related:
- implements: RFC-020
---




## Context

When `lazyspec fix` renumbers a document to resolve a numbering conflict, other documents may reference the old path in their `related` frontmatter entries or `@ref` body directives. These references must be cascaded to the new path, otherwise links break silently. For subfolder documents (e.g. RFCs with child stories), renaming the directory changes paths for the parent and all children, so the cascade must cover those too.

## Acceptance Criteria

- **Given** document A has a `related` entry targeting document B's old path
  **When** document B is renumbered by the fix command
  **Then** document A's `related` entry is rewritten to document B's new path

- **Given** document A contains a `@ref` body directive pointing at document B's old path
  **When** document B is renumbered by the fix command
  **Then** the `@ref` directive in document A's body is rewritten to document B's new path

- **Given** a subfolder document (parent + children) is renumbered, changing its directory name
  **When** the fix command runs
  **Then** all `related` entries and `@ref` directives referencing child document paths under the old directory are also updated to the new directory paths

- **Given** the `--dry-run` flag is passed
  **When** the fix command would cascade reference updates
  **Then** no files on disk are modified, but the JSON output still includes all `ReferenceUpdate` entries that would be applied

- **Given** one or more references are updated (or would be updated in dry-run)
  **When** the fix command returns JSON output
  **Then** the `references_updated` field in `ConflictFixResult` is populated with `ReferenceUpdate` entries containing `file`, `field` ("related" or "body"), `old_value`, and `new_value`

- **Given** no documents reference the renamed document's old path
  **When** the fix command runs
  **Then** `references_updated` is an empty array and no files are modified

## Scope

### In Scope

- Scanning all documents in the store for `related` frontmatter entries that match the old path
- Rewriting matched `related` entries to the new path
- Scanning all document bodies for `@ref` directives that match the old path
- Rewriting matched `@ref` directives to the new path
- Handling subfolder documents: cascading updates for both parent and child paths when a directory is renamed
- Populating `references_updated` in `ConflictFixResult` JSON output
- Respecting the `--dry-run` flag (report but don't write)

### Out of Scope

- Conflict detection and file/directory renaming (Story 1)
- TUI or `show` command changes (Story 3)
- Validation diagnostics for duplicate IDs (Story 4)
- Updating references in non-lazyspec files (only the document store is scanned)
