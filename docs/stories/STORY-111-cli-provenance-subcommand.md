---
title: CLI provenance subcommand
type: story
status: accepted
author: jkaloger
date: 2026-04-29
tags: []
related:
- implements: RFC-039
---



## Context

RFC-039 introduces a `provenance` frontmatter field for citing sources of truth on lazyspec documents. This story delivers the CLI surface for managing those citations: add, remove, and list. All operations mutate frontmatter only and reuse the existing frontmatter writer.

## Dependencies

- **Story 1 (Engine field)** — blocking. Assumes `DocMeta.provenance: Vec<String>` is wired through serde load/save.

## In Scope

- New `lazyspec provenance` subcommand group with `add`, `remove`, and `list` subcommands.
- `add <doc> <citation>` appends a citation string to a document's `provenance` list.
- `remove <doc> <citation>` removes an exact-match citation from a document's `provenance` list.
- `list <doc>` prints citations for a single document.
- `list` (no doc arg) prints citations across all documents, grouped by document.
- All four invocations support `--json`.
- Document is identified by either path or shorthand ID, consistent with `lazyspec show`.
- Clear errors for: doc not found, citation not found on remove, empty citation on add.

## Out of Scope

- Engine-level `DocMeta.provenance` field, serde, and load-time validation (Story 1).
- TUI provenance column or detail panel (Story 3).
- Filter flags on the global `list` command.
- Per-AC attribution.
- Validating citation format or reachability.

## Acceptance Criteria

- **Given** a document with no `provenance` field, **when** `lazyspec provenance add <doc> "Alice Smith <alice@example.com>"` runs, **then** the document's frontmatter contains a `provenance` list with that single citation.

- **Given** a document with an existing `provenance` list, **when** `lazyspec provenance add <doc> <new-citation>` runs, **then** the new citation is appended without modifying existing entries.

- **Given** any document, **when** `lazyspec provenance add <doc> ""` runs, **then** the command exits non-zero with an error indicating the citation is empty and the document is unchanged.

- **Given** a document path that does not resolve, **when** any `provenance` subcommand runs, **then** the command exits non-zero with a "doc not found" error.

- **Given** a document whose `provenance` contains a citation, **when** `lazyspec provenance remove <doc> <citation>` runs with an exact match, **then** that citation is removed and other entries are preserved.

- **Given** a document whose `provenance` does not contain the supplied citation, **when** `lazyspec provenance remove <doc> <citation>` runs, **then** the command exits non-zero with a "citation not found" error and the document is unchanged.

- **Given** a document with a populated `provenance` list, **when** `lazyspec provenance list <doc>` runs, **then** all citations for that document are printed.

- **Given** a document with no `provenance` field, **when** `lazyspec provenance list <doc>` runs, **then** the command exits zero and prints an empty result.

- **Given** a repo with multiple documents carrying provenance, **when** `lazyspec provenance list` runs with no doc arg, **then** citations across all documents are printed and grouped by document.

- **Given** any `provenance` subcommand, **when** invoked with `--json`, **then** stdout is valid JSON describing the result (citations, mutated state, or error).

- **Given** a shorthand document ID accepted by `lazyspec show`, **when** the same ID is passed to any `provenance` subcommand, **then** the command resolves the document identically.
