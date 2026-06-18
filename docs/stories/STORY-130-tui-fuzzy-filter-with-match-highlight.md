---
title: TUI fuzzy filter with match highlight
type: story
status: draft
author: jkaloger
date: 2026-06-18
tags: []
related:
- implements: RFC-043
- related-to: ADR-013
---

## Context

The TUI live filter is substring-only: as the user types, it keeps documents whose lowercased title, tags, or path contain the typed text contiguously, in whatever order the index happens to be in. There is no subsequence matching, no relevance ranking, and the body is never consulted, so finding a document means recalling an exact contiguous fragment of its metadata. This slice swaps that filter for the shared engine fuzzy scorer (delivered by STORY-129), so the TUI ranks live results by relevance, matches non-contiguous subsequences, extends coverage to body content, and visually highlights the characters in each row that matched the query. The matcher itself lives in the engine; this story is purely the TUI's consumption of it and the on-screen feedback.

## Acceptance Criteria

- **Given** a query whose characters appear in a document's title in order but not contiguously (e.g. typing `tff` for `tui fuzzy filter`)
  **When** the filter is active in the TUI
  **Then** that document remains in the filtered list, demonstrating subsequence matching that the old substring filter would have dropped.

- **Given** several documents match the current query with differing relevance
  **When** the filtered list is displayed
  **Then** rows are ordered by relevance score descending, with the more relevant document above the less relevant one.

- **Given** the filter is active
  **When** the user adds or removes characters from the query
  **Then** the filtered, relevance-sorted list updates live to reflect the new query without leaving the filter.

- **Given** a query whose characters fuzzy-match only within a document's body and not its title, tags, or path
  **When** the filter is active
  **Then** that document still appears in the filtered list, showing body content is covered by the TUI filter.

- **Given** a document that appears in the filtered list because some of its characters matched the query
  **When** its row is rendered
  **Then** the specific characters that matched are visually highlighted, distinguishing them from the surrounding unmatched text.

- **Given** a query that no document fuzzy-matches
  **When** the filter is active
  **Then** the filtered list is empty, with non-matching documents excluded by the engine's score floor rather than shown unhighlighted.

## Scope

### In Scope

- Replace the TUI live filter's substring (`.contains()`) path with the shared engine fuzzy scorer from STORY-129.
- Sort the live filtered results by relevance score descending, updating as the user types.
- Extend the TUI filter's match surface to cover body content in addition to title, tags, and path. Body is sourced lazily and streamed into the matcher (not loaded eagerly at startup) and cached, per ADR-013, which supersedes ADR-002's frontmatter-only indexing — startup stays fast and body matches appear as bodies load.
- Highlight the matched characters in rendered document rows using the matcher's match indices.

### Out of Scope

- The engine fuzzy matcher, scoring, ranking, and match-index API (STORY-129).
- CLI `search` result ranking and `--json` `score` output (STORY-131).
- The separate link-editor fuzzy filter.
