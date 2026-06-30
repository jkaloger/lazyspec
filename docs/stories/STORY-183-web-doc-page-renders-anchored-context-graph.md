---
title: Web doc page renders anchored context graph
type: story
status: accepted
author: unknown
date: 2026-07-01
tags: []
related:
- implements: RFC-052
---## Value

As a non-technical reviewer reading a doc in the web view, I want to see every
document in that doc's context -- what it implements/targets, what implements or
targets it, and its related peers -- so I can navigate the structure both
directions without a terminal.

## Context

RFC-052 specifies the per-document web page (STORY-177: frontmatter + body +
`@ref`) and a global `/graph` tree (STORY-179). Neither renders the context
*around a single doc* on its own page. This story adds that: `GET /doc/{id}`
shows every document in the doc's anchored context -- chain ancestors, chain
descendants, and related-to peers -- by reusing
`engine::context::resolve_chain(store, id, depth)`, the same per-doc resolver
that backs `lazyspec context` and the TUI relations tab. Its `ResolvedContext`
already returns the three directions: `nodes` (chain ancestors, topo-ordered),
`forward` (docs that implement/target this doc), and `related` (related-to
peers). No new domain logic; the web layer renders what the engine produces.
Config-driven traversal (STORY-169) governs which relations count as chain vs
related. Depends on STORY-177 (the doc page).

## Acceptance Criteria

- **Given** a doc with chain ancestors, chain descendants, and related docs
  **When** `GET /doc/{id}` renders
  **Then** a Context section lists every document from `resolve_chain` --
  `nodes`, `forward`, and `related` -- in the same order `lazyspec context`
  presents them.

- **Given** chain relations (`implements`/`targets` and their inverses)
  **When** the page renders
  **Then** both directions appear: ancestors (what this doc implements/targets, from
  `nodes`) and descendants (what implements/targets this doc, from `forward`).

- **Given** `related-to` edges
  **When** the page renders
  **Then** related docs appear in the Context section (from `related`).

- **Given** any context entry
  **When** the user clicks it
  **Then** they navigate to that document's `/doc/{id}` page.

- **Given** a doc with no relations
  **When** `GET /doc/{id}` renders
  **Then** the page renders without error and the Context section is empty or absent.

## Scope

### In Scope
- Per-doc context resolution on the doc page via `resolve_chain`.
- Context section template + entry links grouped by direction (ancestors,
  descendants, related).

### Out of Scope
- The global `/graph` page (STORY-179).
- Any write/edit affordance (out of RFC-052 entirely).
- New engine traversal logic -- `resolve_chain` already resolves all three
  directions.