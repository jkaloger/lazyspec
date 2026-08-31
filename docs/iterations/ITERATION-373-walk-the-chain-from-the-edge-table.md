---
title: Walk the chain from the edge table
type: iteration
status: draft
author: jack
date: 2026-08-31
tags: []
related:
- implements: STORY-257
- blocks: ITERATION-374
---

## Objective

`resolve_chain` and `resolve_forest` decide whether a link is hierarchy from the triple (source type, relationship, target type) instead of the relationship name alone, so a `targets` edge declared only for iteration -> milestone stops making every other `targets` link hierarchy.

## Satisfies

STORY-257 AC1, AC2. Also closes STORY-257 §Notes' `child_types_for` reverse-index requirement, which carries no AC of its own. AC3 landed in the preceding iteration; AC4, AC5, AC6 deferred -- see Out of scope.

## Context

- Story + ACs: STORY-257
- Why traversal is a property of the triple: RFC-067 §Problem.3
- What per-edge traversal does and does not buy -- precise where a row is spent, blanket where it is not: RFC-067 §"The traversal cost, stated plainly"
- `chain_relationships: Vec<String>` cannot survive as a flat name list; the walk needs from/to types at each hop: ADR-030 §Consequences, STORY-257 §Notes
- Touch:
  - `src/engine/store.rs` -- `Store.chain_relationships` and the filter in `load_with_fs` that builds it from `RelationshipDef.traversal`
  - `src/engine/context.rs` -- the four chain reads: `resolve_chain`'s upward BFS, its `forward` reverse-link filter, `merge_declared_related`'s chain exclusion, and `chain_parents` (which feeds `resolve_forest`, `resolve_forest_anchored`, `resolve_forest_by_tag`)
  - `src/engine/prompt.rs` `child_types_for` -- derives child types by scanning `config.rules` for `parent == doc_type`
  - `README.md` §`[[relationships]]` traversal paragraph and §`[[edges]]`

**The decision this slice has to make.** The two declarations must not union. If they did, AC1 could not hold: `targets` carries `traversal = "chain"` on `[[relationships]]` in this project, so a union would keep making every `targets` link hierarchy no matter what the edge table says. Take the narrower rule -- an edge row declaring `traversal` for relationship X suppresses `RelationshipDef.traversal` for X entirely, and a relationship no edge row assigns a role to keeps its global marker. This is deliberately *not* the coexistence rule the README states for `[[rules]]` and `[[edges]]` ("enforced independently, neither suppressing the other"), because findings can stack and a walk cannot. Neither ADR-030 nor ADR-031 settles this; if a reviewer disagrees, this is the sentence to argue with.

## Tasks

1. Test-first in `src/engine/context.rs`: with an edge `from = "iteration"`, `to = ["milestone"]`, `via = "targets"`, `traversal = "chain"`, no other `targets` edge, and `targets` *also* marked `traversal = "chain"` on `[[relationships]]` -- `resolve_chain` on a story that some document targets yields a chain without that document (AC1), and `resolve_chain` on the iteration yields the milestone (AC2). One fixture, two assertions; the global marker being present is the point of the test, not an accident of it.
2. Replace the flat list with a triple predicate on `Store`: given a source type, a relationship name and a resolved target type, does this walk as chain. Seed it once at load from `config.edges` (reusing `EdgeDef`'s selector match predicate from ITERATION-369) plus, for relationships no edge row assigns a role to, the legacy `RelationshipDef.traversal`. Whether the index is materialised eagerly or the predicate recomputed per call is left open by STORY-257 §Notes -- pick one and say why in a comment; `resolve_forest` calls it once per relation per document, so a per-call linear scan of rows is not obviously wrong at this row count.
3. Keep the `[[rules]]` checkers frozen. `src/engine/validation.rs` reads `store.chain_relationships` in four places (the rejected-parent and superseded-parent checks, `OrphanedAcceptance`, `MissingParentLink`, and the child-count rule). Those implement `[[rules]]`, whose "any chain relationship" semantics are the defect RFC-067 §Problem.1 describes and STORY-259 deletes. Leave the flat list in place for them and retarget its doc comment to say it now serves `[[rules]]` only. Making those checkers triple-aware here would change `validate` output and break AC5's behaviour-preservation claim.
4. Route the four chain reads in `context.rs` through the predicate, passing both endpoint types. Each site already resolves both documents, so no site needs new lookups.
5. Build the reverse index STORY-257 §Notes calls for and point `child_types_for` at it: a type's child types are the `from` types of chain-traversal edges whose `to` selector admits it, unioned with the existing `[[rules]]` derivation while `[[rules]]` still exists. Test both halves -- a project declaring only rules and a project declaring only edges must each produce child types.
6. Test the blanket case explicitly: `from = "*"`, `to = "*"`, `via = "implements"`, `traversal = "chain"` reproduces today's whole-store forest exactly. This is RFC-067 §"The traversal cost, stated plainly" made executable -- the row that buys no precision and exists to keep the config short.
7. README: §`[[edges]]` states that `traversal` on a row makes the edge walk for that triple only and suppresses the relationship's global marker; §`[[relationships]]` states `traversal` there is the blanket fallback for relationships no edge assigns a role to.

## Out of scope

- The related neighbourhood (AC4) -> next iteration. `related_relationships` stays a flat name list; chain being triple-aware while related is not is a genuine mixed state, and the next slice closes it rather than this one widening.
- `graph.rs`'s `related_annotations`, which filters on a hardcoded `"related-to"` literal -> next iteration.
- Surface parity assertions (AC6) -> the last iteration on this story. The CLI, TUI and web view all route through `resolve_chain`/`resolve_forest`, so they inherit this change; proving that is a separate slice's job, not a reason to skip it.
- `fix --config` migration (AC5) -> STORY-258. Retiring `RelationshipDef.traversal` and `[[rules]]` -> STORY-259.

## Principles / conventions

`lazyspec convention` and the dictums it lists. Dictum 3: the predicate is an engine type; the CLI, TUI and web view call the walk, never re-derive it. Dictum 6: one predicate serving both the chain walk and `child_types_for` is the second use that earns it.

## Verification

`cargo run -- context STORY-257 --json` and `cargo run -- context --anchor iteration --json` are byte-identical before and after on this repo, which declares no edges -- here the legacy fallback is the entire behaviour, and any diff is a regression.
