---
title: Search route over engine search
type: story
status: complete
author: jkaloger
date: 2026-06-30
tags: []
related:
- implements: RFC-052
---## Context

RFC-052 calls for a search route that reuses the same engine search backing `lazyspec search`. This story renders `GET /search?q=` over that engine API, returning a result fragment suitable for htmx so the document list can search-as-you-type. No new search semantics; the web layer is a thin adapter over the engine's existing search. Depends on STORY-176.

## Acceptance Criteria

- **Given** a fixture project and a query `<term>`
  **When** `GET /search?q=<term>` is requested
  **Then** the ordered list of document ids in the response equals the ordered ids returned by the engine search backing `lazyspec search` for the same `<term>` (golden comparison against the CLI output).

- **Given** the document list page
  **When** the user types in the search box
  **Then** htmx issues `GET /search?q=` and replaces the result region in place as the query changes, without a full page reload.

- **Given** an empty `q` parameter
  **When** `GET /search?q=` is requested
  **Then** the route returns the full unfiltered document list (the same content as `GET /`'s list region), not an error.

- **Given** a query matching no documents
  **When** the search runs
  **Then** the response renders an empty-result state.

## Scope

### In Scope

- `GET /search?q=` route over the engine search API.
- htmx wiring from the list page search input to the route.
- Result fragment template reusing the list-row rendering from STORY-176.
- Empty-query and no-match handling.

### Out of Scope

- Any change to engine search semantics or ranking.
- Faceted/advanced filters beyond the status/tag filters of STORY-176.
- Full-text indexing or a search backend (engine search is reused as-is).
