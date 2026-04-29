---
title: TUI provenance column and detail
type: story
status: draft
author: jkaloger
date: 2026-04-29
tags: []
related:
- implements: RFC-039
---


## Context

RFC-039 adds an optional `provenance` frontmatter field on every lazyspec
document type, recording the real-world sources behind a document (people,
workshops, statutes, etc.). With Story 1 landing the engine field and Story 2
landing the CLI, the TUI needs to surface provenance so users can scan and
inspect citations alongside existing metadata like author.

## In Scope

- A `Provenance` column in TUI document list views, positioned alongside the
  existing `Author` column.
- Comma-joined rendering of provenance entries, truncated with an ellipsis
  when wider than the column.
- Empty cell when a document has no provenance entries.
- Provenance entries listed in the document detail panel when present.
- Read-only behaviour: no add/edit/delete affordance from the TUI.

## Out of Scope

- Engine `DocMeta.provenance` field, serde, and validation (Story 1).
- `lazyspec provenance` CLI subcommands (Story 2).
- Editing provenance entries from the TUI (deferred).
- Search or filter UI keyed on provenance (deferred).

## Dependencies

- Story 1: Engine `DocMeta.provenance: Vec<String>` field must exist and be
  populated from frontmatter before this story is implementable.

## Acceptance Criteria

- **Given** a list view of documents in the TUI,
  **when** the view renders,
  **then** a `Provenance` column appears alongside the existing `Author`
  column.

- **Given** a document with multiple provenance entries,
  **when** its row is rendered in a list view,
  **then** the `Provenance` cell shows the entries joined by commas.

- **Given** a document whose provenance entries exceed the column width,
  **when** its row is rendered,
  **then** the cell content is truncated and ends with an ellipsis.

- **Given** a document with no provenance entries,
  **when** its row is rendered,
  **then** the `Provenance` cell is empty.

- **Given** a document with provenance entries,
  **when** the user opens its detail panel,
  **then** each provenance entry is listed in the panel.

- **Given** a document with no provenance entries,
  **when** the user opens its detail panel,
  **then** no provenance section is shown (or it is rendered empty,
  consistent with other optional list fields).

- **Given** the document detail panel is open on a document with provenance,
  **when** the user attempts to add, edit, or remove a provenance entry from
  the TUI,
  **then** no editing affordance is available and the entries remain
  unchanged.
