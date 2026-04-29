---
title: Engine support for provenance frontmatter
type: story
status: accepted
author: jkaloger
date: 2026-04-29
tags: []
related:
- implements: RFC-039
---



## Context

RFC-039 introduces a free-form `provenance` frontmatter field that cites the real-world sources of truth behind a document (people, workshops, statutes, etc.). This story delivers the engine-level foundation: the `DocMeta` field, serde round-trip, load-time validation, and backwards compatibility. CLI surface and TUI rendering are handled in subsequent stories.

## In Scope

- Add a `provenance` field on `DocMeta` typed as a list of free-form strings, defaulting to empty.
- Read the field from frontmatter YAML and write it back unchanged via the existing frontmatter loader/writer.
- Reject empty-string entries at load time with a clear error message.
- Preserve backwards compatibility: documents without a `provenance` field load with an empty default.

## Out of Scope

- `lazyspec provenance add/remove/list` CLI subcommand group (Story 2).
- TUI Provenance column and detail panel rendering (Story 3).
- Extending search/index to cover provenance entries.
- Per-requirement (per-AC) attribution.
- Typed source kinds, source documents as first-class types, or citation-format validation.

## Acceptance Criteria

- **Given** a document whose frontmatter contains a `provenance` list with several free-form string entries,
  **When** the document is loaded by the engine,
  **Then** the resulting `DocMeta` exposes those entries in the same order as written.

- **Given** a `DocMeta` with a populated `provenance` list,
  **When** the document is written back through the frontmatter writer without other changes,
  **Then** the rewritten file contains the same `provenance` entries in the same order, with no entries added, removed, or reformatted in a lossy way.

- **Given** a document whose frontmatter omits the `provenance` field entirely,
  **When** the document is loaded,
  **Then** loading succeeds and `DocMeta.provenance` is an empty list.

- **Given** a document whose frontmatter contains a `provenance` list including an empty-string entry,
  **When** the document is loaded,
  **Then** loading fails with an error that identifies the offending document and indicates that empty provenance entries are not permitted.

- **Given** a document whose frontmatter has `provenance: []`,
  **When** the document is loaded,
  **Then** loading succeeds and `DocMeta.provenance` is an empty list.

- **Given** a `DocMeta` with an empty `provenance` list,
  **When** the document is written back through the frontmatter writer,
  **Then** the resulting file remains valid and re-loads to a `DocMeta` whose `provenance` is empty.

- **Given** any document type supported by the engine (RFC, Story, Iteration, Audit, ADR, etc.),
  **When** that document carries a `provenance` list in frontmatter,
  **Then** the field round-trips through load and save identically across every document type.
