---
title: Search route over engine search
type: iteration
status: accepted
author: jkaloger
date: 2026-06-30
tags: []
related:
- implements: STORY-178
---

## Objective

Add `GET /search?q=` to the web layer as a thin adapter over `Store::search`, returning an htmx-swappable list fragment that matches the engine search ordering exactly.

## Satisfies

STORY-178 (all ACs): golden id-order parity with `lazyspec search`, htmx search-as-you-type, empty-query falls back to the full list, no-match empty state.

## Context

- Story + ACs: STORY-178 (docs/stories/STORY-178-search-route-over-engine-search.md)
- RFC: RFC-052 §Routes, §Layering (web -> engine only)
- Engine search (reuse as-is): `Store::search(&self, query: &str, fs: &dyn FileSystem) -> Vec<SearchResult>` in src/engine/store.rs. Results are already ordered (date asc, then path) and deduped to one match per doc. `SearchResult` carries `doc: &DocMeta` (has `.id`), `match_field`, `snippet`.
- Golden reference for ordering: src/cli/search.rs (`run_json`) — same engine call, same order.
- Web skeleton (dependency, STORY-176): src/web/ — the `Arc<Store>` setup, the axum `Router`, the `GET /` list route, the list-row template/partial, and the htmx setup all land there. Reuse the list-row rendering; do not reimplement it.
- Touch: src/web/routes.rs (register `GET /search`, add handler), src/web/render.rs (search-results fragment over the existing list-row partial), the `GET /` list template (add the htmx search input targeting the result region).

## Tasks

1. Add the `GET /search` route and handler in src/web/routes.rs: extract `q` from the query string (absent or empty `q` is valid, not an error), share the existing `Arc<Store>`, and obtain the same `FileSystem` the skeleton uses.
2. Handler logic: on empty/absent `q`, render the full unfiltered list fragment (identical content to the `GET /` list region). Otherwise call `store.search(q, fs)` and render the results fragment over the matched docs, preserving the engine's returned order. On zero results, render the empty-result state.
3. Build the search-results fragment in src/web/render.rs by reusing STORY-176's list-row partial for each result; do not introduce new row markup or new ordering.
4. Wire the search input on the `GET /` list page: htmx `GET /search?q=` on input, targeting and replacing the result region in place (no full reload).
5. Test-first: add a golden test asserting the ordered doc ids from `GET /search?q=<term>` equal those from the engine search path (compare against `cli::search::run_json` or `Store::search` directly) for a fixture project; plus cases for empty `q` (full list) and a no-match query (empty state).

## Out of scope

- Any change to engine search semantics, ranking, or the `SearchResult` shape.
- Faceted/advanced filters beyond STORY-176's status/tag filters.
- Full-text indexing or a search backend.
- Auth/session concerns — search lands on the loopback skeleton (STORY-181 gates hosted).

## Principles / conventions

- CLAUDE.md (project): run the dev binary via `cargo run`; `--json` for machine-readable output; update the README if the CLI surface changes (no CLI surface change expected here).
- RFC-052 layering invariant: src/web/ imports only from `engine`, never `cli` or `tui`. The golden test may read CLI output as a comparison oracle but the route must not depend on the cli module.
- Match the engine's result order exactly — never re-sort in the web layer.

## Verification

- `GET /search?q=<term>` id order is byte-for-byte the engine search order for the same term over a fixture (golden).
- `GET /search?q=` (empty) returns the same rows as the `GET /` list region.
- A no-match query renders the empty-result state, not an error or a stale list.

