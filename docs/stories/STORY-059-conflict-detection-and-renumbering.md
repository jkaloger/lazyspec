---
title: Conflict detection and renumbering
type: story
status: accepted
author: jkaloger
date: 2026-03-13
tags: []
related:
- implements: docs/rfcs/RFC-020-fix-command-numbering-conflict-resolution.md
---



## Context

`next_number()` assigns document numbers by scanning the local filesystem. When two contributors on separate branches both create a document of the same type, they get the same number. After merge, the project has duplicate IDs (e.g. two `RFC-020` documents), causing `resolve_shorthand` to return arbitrary results and relationships to silently break.

This story extends the `fix` command to detect and resolve these numbering conflicts. It covers detection, priority logic, disk rename, and frontmatter title update. Reference cascade across other documents is handled separately in Story 2.

## Acceptance Criteria

- **Given** a project with two flat-file documents sharing the same ID (e.g. `RFC-020-foo.md` and `RFC-020-bar.md`)
  **When** `lazyspec fix` is run
  **Then** the document with the earlier `date` frontmatter value keeps its number, and the other is renumbered to the next available number for that type

- **Given** two conflicting documents with identical `date` values
  **When** `lazyspec fix` is run
  **Then** the document with the earlier filesystem mtime keeps its number

- **Given** a conflicting document that is renumbered
  **When** the rename completes
  **Then** the file on disk is renamed from the old ID prefix to the new one (e.g. `RFC-020-bar.md` becomes `RFC-021-bar.md`)

- **Given** a conflicting document whose frontmatter `title` contains the old ID prefix
  **When** the document is renumbered
  **Then** the old ID prefix in the title is replaced with the new one

- **Given** a subfolder document with a numbering conflict (e.g. `RFC-020-bar/index.md`)
  **When** `lazyspec fix` is run
  **Then** the entire directory is renamed (e.g. `RFC-020-bar/` becomes `RFC-021-bar/`) and the `index.md` and children move with it

- **Given** three or more documents sharing the same ID
  **When** `lazyspec fix` is run
  **Then** each losing document is renumbered to a distinct next-available number, preserving the oldest-wins rule

- **Given** `lazyspec fix --dry-run` is run on a project with numbering conflicts
  **When** the command completes
  **Then** no files are renamed or modified on disk, and the output reports what would change

- **Given** `lazyspec fix --json` is run on a project with numbering conflicts
  **When** the command completes
  **Then** the output matches the `FixOutput` shape containing `field_fixes` and `conflict_fixes` arrays

- **Given** `lazyspec fix --json` resolves a conflict
  **When** the JSON output is inspected
  **Then** each `ConflictFixResult` contains `old_path`, `new_path`, `old_id`, `new_id`, and `written` fields

- **Given** a project with no numbering conflicts
  **When** `lazyspec fix` is run
  **Then** `conflict_fixes` is an empty array and existing field-fix behaviour is unchanged

## Scope

### In Scope

- Building an ID-frequency map by scanning all loaded documents during `fix`
- Oldest-wins priority logic (date frontmatter, mtime tiebreak)
- File rename on disk for flat-file documents
- Directory rename on disk for subfolder documents
- Frontmatter title update when the title contains the old ID prefix
- `FixOutput` JSON shape with `field_fixes` and `conflict_fixes`
- `ConflictFixResult` struct with `old_path`, `new_path`, `old_id`, `new_id`, `written`
- `--dry-run` support for conflict resolution
- Handling three-or-more-way conflicts

### Out of Scope

- Reference cascade: rewriting `related` targets and `@ref` body directives in other documents (Story 2)
- Graceful degradation in `resolve_shorthand`, `show`, and TUI for duplicate IDs (Story 3)
- Validation diagnostic rule for duplicate IDs (Story 4)
- Prevention at `create` time (locking, reserving numbers)
- `ReferenceUpdate` and `references_updated` field within `ConflictFixResult` (Story 2 concern)
