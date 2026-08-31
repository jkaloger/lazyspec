---
title: Walk the related neighbourhood from the edge table
type: iteration
status: draft
author: jack
date: 2026-08-31
tags: []
related:
- implements: STORY-257
- blocks: ITERATION-375
---

## Objective

The related neighbourhood is decided per triple too, and the graph view's cross-cutting annotations stop filtering on a hardcoded `related-to` literal.

## Satisfies

STORY-257 AC4. AC1, AC2, AC3 landed in the preceding iterations; AC5, AC6 deferred -- see Out of scope.

## Context

- Story + ACs: STORY-257
- One wildcard `related-to` row is the shape the starter config keeps, and why enumerating the alternative is absurd: ADR-031 §Context, §Consequences
- Relations carrying no traversal role must still surface at one hop -- the BUG-013 fix `merge_declared_related` exists for: `src/engine/context.rs` doc-comment
- Touch:
  - `src/engine/store.rs` -- `related_relationships`, the sibling of the list the preceding slice left behind
  - `src/engine/context.rs` -- `resolve_chain`'s related BFS (both the forward-link and reverse-link neighbour filters) and `merge_declared_related`
  - `src/engine/graph.rs` -- `related_annotations` filters on `rel.as_str() != "related-to"`, a literal that has never consulted the config at all
  - `README.md` §`[[edges]]`

## Tasks

1. Test-first (AC4): a `from = "*"`, `to = "*"`, `via = "related-to"`, `traversal = "related"` row produces the same `related` set -- same members, same `distance`, same `via` -- that the same documents produce today under `[[relationships]]`'s `traversal = "related"`. Assert on the resolved output, not on the field it read; AC4 is a claim about output equivalence.
2. Extend the preceding slice's predicate to the related role and route `context.rs`'s related-BFS filters through it.
3. `merge_declared_related` filters on the chain role only, so it needs no related-role change -- prove that with a test rather than by reading. A relation whose triple carries no role at all must still surface in the related section at one hop; that is BUG-013's fix and it must not regress when the role source changes.
4. Replace `related_annotations`' `"related-to"` literal with the related-role predicate. This is a real behaviour change for any project whose related relationship is named something else -- today the graph silently annotates nothing for it. Cover it: a project whose only `traversal = "related"` relationship is `mentions` gets annotations.
5. Delete `Store.related_relationships` once nothing reads it. Unlike `chain_relationships`, no `[[rules]]` checker depends on it -- verify that before deleting rather than assuming the symmetry.
6. README §`[[edges]]`: `traversal = "related"` on a row scopes the neighbourhood to that triple, and the graph view's cross-cutting annotations follow the same declaration as `context`.

## Out of scope

- `Store.chain_relationships` -- still serving the `[[rules]]` checkers, and dies with them in STORY-259.
- Parity assertions across the three surfaces (AC6) -> next iteration. In particular the web view layers `doc.related` itself in `src/web/render.rs` rather than calling `merge_declared_related`; that divergence is the next slice's question. Do not pre-empt it here.
- `fix --config` migration (AC5) -> STORY-258.

## Principles / conventions

`lazyspec convention` and the dictums it lists. Dictum 3: the graph's annotation predicate is engine-side, shared with the walk, not re-derived in `graph.rs`.

## Verification

`cargo run -- context STORY-257 --json | jq .related` is byte-identical before and after on this repo, and the TUI graph view's cross-cutting annotations are unchanged for a document that carries `related-to` links.
