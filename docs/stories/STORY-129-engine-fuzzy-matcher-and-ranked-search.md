---
title: Engine fuzzy matcher and ranked search
type: story
status: in-progress
author: jkaloger
date: 2026-06-18
tags: []
related:
- implements: RFC-043
- related-to: ADR-013
---

## Context

Engine search is substring-only: it lowercases the query and target and asks whether one contains the other, returning the first matching field per document with no notion of relevance. Results come back in HashMap iteration order, so the same query can list documents differently between runs, and finding a document requires recalling an exact contiguous substring. This slice replaces that with fuzzy subsequence matching and relevance ranking in the engine, so a single scoring implementation can later back both CLI search and the TUI filter. Each document is scored across its title, tags, path, and full body; the best-scoring field is reported; results come back ranked by score with a deterministic tie-break, and documents that do not match at all are dropped. This is the foundation story: it owns only the engine matcher and the shape of `SearchResult`, leaving CLI output and the TUI to consume it.

## Acceptance Criteria

- **Given** a query whose characters appear in a document's title in order but not as a contiguous substring (e.g. query `enfz` against `engine fuzzy`)
  **When** the engine `search` runs
  **Then** that document is returned as a match, demonstrating subsequence (non-contiguous) matching that the old substring search would have missed.

- **Given** multiple documents that all match a query with differing relevance
  **When** the engine `search` runs
  **Then** results are ordered by score descending, with the more relevant document appearing before the less relevant one.

- **Given** two documents that match a query with equal scores
  **When** the engine `search` runs repeatedly
  **Then** their relative order is identical across runs, fixed by a deterministic tie-break (e.g. by path), so equal-score ordering is reproducible.

- **Given** a corpus containing documents that the query does not fuzzy-match at all
  **When** the engine `search` runs
  **Then** those non-matching documents are excluded by a score floor and never appear in the results.

- **Given** a query whose characters match only within a document's body text and not its title, tags, or path
  **When** the engine `search` runs
  **Then** that document is still returned as a match, with its `match_field` reflecting the body.

- **Given** a document that matches the query in more than one field
  **When** the engine `search` runs
  **Then** exactly one result is returned for that document, carrying the score of its best-scoring field and a `match_field` naming that best field.

- **Given** any returned search result
  **When** the result is inspected
  **Then** it exposes a `score` field reflecting the document's fuzzy relevance for the query.

## Scope

### In Scope

- Add the full `nucleo` crate (streaming fzf-style matcher: injector + background workers + ranked snapshots), per ADR-013. This supersedes the lighter `nucleo-matcher` originally sketched in RFC-043, so the same matcher can later back the TUI's lazy, streamed body matching.
- Rewrite the engine `search` to fuzzy-match and score each document across title, tags, path, and full body.
- Aggregate to one result per document using its best-scoring field, with `match_field` reflecting that field.
- Return results sorted by score descending with a deterministic, stable tie-break (e.g. by path).
- Apply a score floor so documents with no fuzzy match are excluded.
- Add a `score` field to `SearchResult`.
- Unit tests covering: non-contiguous subsequence match, ranking order, tie-break determinism, score-floor exclusion of non-matches, and body matches surfacing.

### Out of Scope

- CLI `search` `--json` `score` output and CLI result ordering (STORY-131).
- TUI filter consuming the scorer and matched-character highlighting (STORY-130).
