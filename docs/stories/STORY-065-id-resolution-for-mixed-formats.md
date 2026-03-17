---
title: ID resolution for mixed formats
type: story
status: accepted
author: agent
date: 2026-03-16
tags: []
related:
- implements: docs/rfcs/RFC-027-sqids-document-numbering.md
---



## Context

Document IDs are currently assumed to be numeric (`RFC-022`, `STORY-064`). With the introduction of sqids-based numbering (RFC-027), IDs can also be short alphanumeric strings like `RFC-k3f`. The ID extraction and resolution logic needs to handle both formats so that sqids and incremental documents can coexist in the same directory.

## Acceptance Criteria

- **Given** a document with a numeric ID (e.g. `RFC-022-some-title.md`)
  **When** `extract_id_from_name` is called
  **Then** it returns `RFC-022`

- **Given** a document with an alphanumeric sqids ID (e.g. `RFC-k3f-some-title.md`)
  **When** `extract_id_from_name` is called
  **Then** it returns `RFC-k3f`

- **Given** a directory containing both `RFC-022-foo.md` and `RFC-k3f-bar.md`
  **When** resolving shorthand `RFC-022`
  **Then** it resolves to `RFC-022-foo.md`

- **Given** a directory containing both `RFC-022-foo.md` and `RFC-k3f-bar.md`
  **When** resolving shorthand `RFC-k3f`
  **Then** it resolves to `RFC-k3f-bar.md`

- **Given** a document with a multi-segment alphanumeric ID (e.g. `STORY-a2b-some-title.md`)
  **When** `extract_id_from_name` is called
  **Then** it returns `STORY-a2b` (the prefix plus the first alphanumeric segment after it)

- **Given** shorthand input with no matching document
  **When** resolving the shorthand
  **Then** the existing error behavior is preserved

- **Given** an `index.md` inside a folder named `RFC-k3f-some-title`
  **When** `extract_id` is called on its path
  **Then** it correctly extracts `RFC-k3f` from the folder name

## Scope

### In Scope

- Update `extract_id_from_name` to recognize alphanumeric ID segments (not just numeric)
- Update `resolve_shorthand` to match against alphanumeric IDs
- Coexistence of sqids and incremental documents in the same directory
- Folder-based documents with sqids IDs (index.md inside `RFC-k3f-slug/`)

### Out of Scope

- Numbering strategy implementation, `NumberingStrategy` enum, config parsing (Story A)
- `fix --renumber` migration between formats (Story C)
- Sqids crate integration or ID generation
- Validation of sqids config (`min_length`, `salt`, `alphabet`)
