---
title: CLI search ranking and score output
type: story
status: in-progress
author: jkaloger
date: 2026-06-18
tags: []
related:
- implements: RFC-043
---

## Context

CLI `search` is a thin surface over the engine's `search()`. Today its `--json` output emits one object per result (the document plus `match_field` and `snippet`) but carries no relevance signal, and results come back in whatever order the engine produces. Once the engine returns scored, rank-sorted results (STORY-129), the CLI should expose that signal: surface the per-result `score` in `--json` and present results best-match-first in both `--json` and human-readable output. Because agents may want the full result set, the CLI applies no result cap. Existing affordances — the `--type` filter, `match_field`, and `snippet` — must keep working unchanged.

## Acceptance Criteria

- **Given** the engine returns scored results for a query
  **When** `search --json` is run
  **Then** each result object includes a numeric `score` field alongside the existing document fields, `match_field`, and `snippet`.

- **Given** a query that matches several documents with differing relevance
  **When** `search --json` is run
  **Then** the result array is ordered by `score` descending (highest relevance first).

- **Given** a query that matches several documents with differing relevance
  **When** `search` is run in human-readable mode
  **Then** results are printed in `score`-descending order (highest relevance first).

- **Given** a query that matches documents of multiple types
  **When** `search --type <type>` is run (in either `--json` or human-readable mode)
  **Then** only results of that type are returned, and the surviving results remain ordered by `score` descending.

- **Given** a query that matches documents
  **When** `search --json` is run
  **Then** every matching result is present (the score floor that drops non-matches lives in the engine), with no hard cap applied by the CLI.

- **Given** a query that matches documents
  **When** `search` is run in either mode
  **Then** each result still reports the same `match_field` and `snippet` values it did before scoring was added.

## Scope

### In Scope

- Add a per-result numeric `score` field to CLI `search --json` output.
- Order CLI results by `score` descending in both `--json` and human-readable output.
- Preserve existing behavior: the `--type` filter, `match_field`, and `snippet` remain present and correct.
- Apply no hard result cap to `--json` output (rely on the engine's score floor to drop non-matches).

### Out of Scope

- The engine fuzzy matcher and scoring implementation that produces the `score` and rank order (STORY-129).
- TUI filter consumption of the scorer and match highlighting (STORY-130).
