---
title: Web doc view renders anchored context graph both ways
type: iteration
status: complete
author: unknown
date: 2026-07-01
tags: []
related:
- implements: STORY-183
---## Objective
Render the doc's context (chain ancestors, chain descendants, related peers) as a
Context section on `GET /doc/{id}`, mirroring `lazyspec context`.

## Satisfies
STORY-183 AC1-AC5 (single render slice; whole story).

## Context
- Story + ACs: STORY-183
- Spec: docs/rfcs/RFC-052-read-only-web-view-for-lazyspec-docs.md (Design, Routes)
- Per-doc resolver: `engine::context::resolve_chain(store, id, depth)` ->
  `ResolvedContext { target, nodes, forward, related }`. Reference impl:
  src/cli/context.rs (`run_json`/`run_human`) and the TUI relations tab.
  `nodes` = chain ancestors (topo-ordered, unbounded BFS up parent-child rels);
  `forward` = docs linking to target via a chain rel (descendants, one hop);
  `related` = related-to peers (followed up to `depth` hops, default 1).
- Traversal classes (chain vs related): STORY-169 (config-driven), config
  `relationships`.
- Conventions: docs/convention (layering: web -> engine only).
- Touch: src/web/routes.rs (`doc_page`: call `resolve_chain(&store, &doc.id, 1)`),
  src/web/render.rs (`DocPage`: add the three context groups), the askama doc
  template (Context section), src/web/routes.rs tests.

## Tasks
1. In `doc_page`, after resolving `doc`, call
   `resolve_chain(&store, &doc.id, 1)` (depth 1, matching `lazyspec context`
   default). Handle the `Err` (unknown id already 404s earlier) without a 500.
2. Extend `DocPage` (render.rs) with three ordered groups from `ResolvedContext`:
   ancestors (`nodes`, excluding the target itself), descendants (`forward`),
   related (`related`). Each entry carries id, type, status, title.
3. Add the Context section to the doc template: the three groups labelled by
   direction, each entry linking to `/doc/{id}`; render nothing when all three
   are empty.
4. Test: a doc with an ancestor, a descendant, and a related peer yields all three
   in the right groups; a doc with no relations renders cleanly (no Context
   section, no error).

## Out of scope
- Global `/graph` page (STORY-179).
- Edit affordances; new engine traversal logic.

## Verification
A doc that both implements a parent and is implemented-by a child shows the parent
under ancestors and the child under descendants, plus any related-to peer under
related -- matching `lazyspec context <id>` for the same doc.