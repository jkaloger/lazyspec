---
title: Fix numbering format conversion
type: story
status: accepted
author: agent
date: 2026-03-16
tags: []
related:
- implements: RFC-027
---




## Context

When a team switches their numbering strategy from incremental to sqids (or vice versa), existing documents retain their original IDs. This works fine for coexistence, but teams that want a clean cutover need a way to bulk-convert document IDs. The `lazyspec fix` command already handles frontmatter repair and numbering conflicts, making it the natural home for format conversion.

Renaming files is only half the problem. Every `related` frontmatter reference pointing at a renamed document must be updated in the same pass, or relationships silently break.

## Acceptance Criteria

- **Given** a project with incremental-numbered documents
  **When** `lazyspec fix --renumber sqids` is run
  **Then** each document is renamed with its sqids-encoded ID (e.g. `RFC-022-slug.md` becomes `RFC-k3f-slug.md`)

- **Given** a project with sqids-numbered documents
  **When** `lazyspec fix --renumber incremental` is run
  **Then** each document is renamed with its zero-padded numeric ID decoded from the sqids value

- **Given** a project with multiple document types
  **When** `lazyspec fix --renumber sqids --type rfc` is run
  **Then** only RFC documents are renamed; other types are left unchanged

- **Given** documents with `related` frontmatter referencing a document that will be renamed
  **When** the renumber operation completes
  **Then** all `related` paths across the project are updated to reflect the new filenames

- **Given** any renumber operation
  **When** `--dry-run` is passed
  **Then** no files are modified on disk, and the output lists every rename and reference update that would occur

- **Given** non-lazyspec files (READMEs, wikis) that reference renamed documents
  **When** the renumber operation completes
  **Then** a summary of external references that could not be auto-updated is printed

- **Given** a project with mixed incremental and sqids documents for the same type
  **When** `lazyspec fix --renumber sqids` is run
  **Then** only incremental-numbered documents are converted; already-sqids documents are skipped

## Scope

### In Scope

- `lazyspec fix --renumber sqids|incremental` command
- `--type` flag to scope conversion to specific document types
- `--dry-run` flag to preview changes without filesystem modifications
- File renames translating between incremental and sqids ID formats
- Cascading updates to all `related` frontmatter paths across the project
- Summary output listing external references that couldn't be auto-updated

### Out of Scope

- Core sqids numbering strategy, `NumberingStrategy` enum, config, and `next_id` logic (Story A)
- ID resolution changes to `extract_id_from_name` and `resolve_shorthand` (Story B)
- Updating `@ref` directives or external links outside the docs directory
