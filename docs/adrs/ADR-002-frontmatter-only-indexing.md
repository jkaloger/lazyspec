---
title: Frontmatter-Only Indexing
type: adr
status: accepted
author: jkaloger
date: 2026-03-04
tags:
- performance
- store
related:
- related-to: RFC-001
---


## Context

The store needs to load all documents on startup for the type counts, document lists, and link graph. Loading full file contents would be slow for large documentation sets.

## Decision

On startup, the store reads only the YAML frontmatter from each file (stopping at the second `---` delimiter). Document body content is loaded lazily via `Store::get_body` when a specific document is selected for preview.

This means the fuzzy search in the TUI operates on frontmatter fields only (title, tags, author), not body content. The CLI `search` command does load body content for full-text search, but this is acceptable since it's a one-shot operation.

## Consequences

- Fast startup regardless of document count or body size
- TUI fuzzy search is limited to frontmatter fields
- Body content is never cached in the store, re-read from disk on each access
- File watching only needs to re-parse frontmatter on change events
