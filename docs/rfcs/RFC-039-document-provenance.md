---
title: "Document Provenance"
type: rfc
status: draft
author: "jkaloger"
date: 2026-04-29
tags: []
---

## Intent

Lazyspec documents capture decisions and requirements. Many originate from real-world sources: stakeholder interviews, design workshops, statutes or regulations. Today there is no machine-readable way to record "this requirement came from X". Tracing a clause back to its source is manual.

Add a free-form frontmatter field on every document that cites sources of truth. Modeled on `author`: a list of strings, surfaced in TUI as a column. KISS.

## Non-goals

- Per-requirement (per-AC) attribution. Doc-level only. Future iteration.
- Typed source kinds (person/workshop/legislation as enum). Free-form strings; the citation text says what it is.
- Sources as first-class lazyspec document types.
- Validating reachability or format of citations.
- TUI editing affordance. Read-only display.

## Design

Add optional frontmatter field `provenance` on all document types. List of free-form strings:

```yaml
provenance:
  - "Alice Smith <alice@example.com>"
  - "2026-Q1 Compliance Workshop"
  - "GDPR Article 17 — https://gdpr-info.eu/art-17-gdpr/"
```

Each entry is one line of human-readable text. No structure beyond that. Empty strings rejected.

### Engine

`DocMeta` gains `pub provenance: Vec<String>`, defaulting to empty. No new module, no enum, no struct. Serde round-trips through existing frontmatter loader/writer (@ref engine/store.rs).

### CLI

New subcommand group:

- `lazyspec provenance add <doc> <citation> [--json]`
- `lazyspec provenance remove <doc> <citation> [--json]`
- `lazyspec provenance list <doc> [--json]`
- `lazyspec provenance list --json` across all docs

All operations mutate frontmatter only. Reuse existing frontmatter writer.

### TUI

Add a `Provenance` column to document list views, alongside `Author`. Renders comma-joined entries, truncated to fit. Empty cell when none. Read-only.

Document detail panel lists entries when present.

### Search

Existing `lazyspec search` matches body and title. Extending to provenance strings is trivial (one more field in the match path); include in story 2 if cheap.

### Validation

- Empty strings rejected at frontmatter load.
- No other rules.

## Stories

1. **Engine support for provenance frontmatter** — `DocMeta` carries `provenance: Vec<String>`, serde round-trip, load-time validation, no CLI surface.
2. **CLI provenance subcommand** — `add` / `remove` / `list` (per-doc and global), all `--json`.
3. **TUI provenance column and detail** — list views show `Provenance` column; detail panel lists entries.

Story 1 blocks 2 and 3. Stories 2 and 3 are parallel.

## ADR candidates

- Frontmatter list vs Source-as-document-type (decided: list).
- Free-form strings vs typed kind enum (decided: free-form, see ADR-006 update).
