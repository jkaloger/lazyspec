---
title: Re-derive the surviving hierarchy findings from the edge table
type: iteration
status: draft
author: Jack Kaloger
date: 2026-09-01
tags: []
related:
- implements: STORY-259
- blocks: ITERATION-384
---

## Objective

This repository's own `.lazyspec.toml` moves to `[[edges]]`, and the five findings that outlive `[[rules]]` -- `RejectedParent`, `SupersededParent`, `OrphanedAcceptance`, `AllChildrenAccepted`, `UpwardOrphanedAcceptance` -- decide what counts as hierarchy from the edge table's chain rows instead of from `ValidationRule::ParentChild` and a flat relationship-name list.

## Satisfies

STORY-259 AC3, in part: it clears the two `ValidationRule::ParentChild` readers whose deletion is *not* subtractive. `hierarchy_from_config` (`src/engine/validation.rs:413-421`) is a `ValidationRule` site AC3 requires gone, and five surviving findings depend on it. AC3 becomes true and greppable in ITERATION-385; AC1 and AC2 land in ITERATION-384, AC4 in ITERATION-383, AC5 in ITERATION-385.

## Context

- Story + ACs: STORY-259
- Why `chain_relationships` cannot stay a flat name list, and that this cascade is the largest single cost of the decision: ADR-030 §Consequences
- This slice is the obligation ITERATION-373 Task 3 deferred by name ("leave the flat list in place for them ... and dies with them in STORY-259"), repeated in ITERATION-374 §Out of scope. The triple predicate it hands over is the one built in ITERATION-373 Task 2, seeded from `config.edges` plus `EdgeDef::matches`
- That the migration is behaviour-preserving is STORY-258 AC5, proved in ITERATION-379. This slice consumes that guarantee rather than re-establishing it
- Touch:
  - `.lazyspec.toml` -- this repo's three `[[rules]]` blocks (`:174-193`) and no `[[edges]]`. Migrated by running `fix --config`, not by hand
  - `src/engine/validation.rs` -- the four `store.chain_relationships` reads: `:468` (`RejectedParent` / `SupersededParent` in `BrokenLinkRule`), `:503` (`OrphanedAcceptance`, which also reads `hierarchy`), `:565` (`MissingParentLink`, which does *not* move -- see Out of scope), `:715` (`AllChildrenAccepted` / `UpwardOrphanedAcceptance` in `StatusConsistencyRule`, which also reads `hierarchy`). Plus `hierarchy_from_config` and its two call sites, `:432` and `:699`
  - `src/engine/store.rs` -- `Store.chain_relationships` (`:41`) and the filter that builds it from `RelationshipDef.traversal` (`:114-123`, assigned at `:135`)
  - The hand-built `Store` fixtures carrying `chain_relationships: vec![...]` in `validation.rs`'s test module (`:1485`, `:1581`, `:1729`, `:1818`, `:2148`) -- each needs an `[[edges]]` row instead, so the fixture states the DAG its assertion depends on
  - `README.md` §`[[relationships]]` traversal paragraph -- it still says chain relationships are what "`parent-child` validation rules and the context chain walk follow"
- **Why the repo config moves first.** The moment hierarchy is read from `[[edges]]`, a config declaring only `[[rules]]` has no hierarchy -- so all five findings would go silent on this repo. Migrating `.lazyspec.toml` in the same slice is what keeps `validate` honest here, and it is why this slice, not ITERATION-384, is where STORY-258's escape route is first depended on.
- **The distinction this slice has to hold.** `hierarchy_from_config` returns type *pairs* and `chain_relationships` returns relationship *names*; the two are ANDed at three of the four sites. One edge row supplies both, so the two lookups collapse into one predicate call -- but `RejectedParent` and `SupersededParent` (`:468`) ask only "is this relationship chain?", with no type-pair condition at all. Routing them through a triple predicate narrows them. Decide explicitly whether that narrowing is wanted and say so in a comment: after this story an edge row is the only declaration of hierarchy left, so "any chain relationship, whatever its endpoints" has nothing left to read.

## Tasks

1. Migrate `.lazyspec.toml` by running `fix --config` (STORY-258), and record the resulting `[[edges]]` in the same commit. Confirm `validate --json` is unchanged across the migration before touching any Rust -- if it is not, the defect is in STORY-258, not here.
2. Test-first in `src/engine/validation.rs`: with a single chain edge row `from = "story"`, `to = ["rfc"]`, `via = "implements"`, `traversal = "chain"` and no `[[rules]]` at all, a story implementing a `rejected` rfc reports `RejectedParent`, and a story linked to that same rfc by a non-chain relationship does not. This is the assertion that proves the derivation moved: today it needs a `[[rules]]` block to fire.
3. Route the three surviving sites through ITERATION-373's chain predicate, passing both endpoint types at each. Every site already has both documents resolved, so none needs a new lookup.
4. Delete `hierarchy_from_config`. `StatusConsistencyRule` iterates `(parent_type, child_type)` pairs as its outer loop, so it needs the forward direction of the edge index -- the child types declared for a parent type. That is the reverse index `child_types_for` got in ITERATION-373; reuse it rather than building a second one.
5. Delete `Store.chain_relationships` and its `load_with_fs` filter if `:565` is no longer its last reader; otherwise leave the field with a doc comment naming ITERATION-384 as its executioner. Do not leave the comment claiming it serves `[[rules]]` generally when one checker remains.
6. Rewrite the hand-built `Store` fixtures to declare edges, asserting the same findings they assert today. This slice must not change what `validate` reports for a config that declares either shape.
7. README: the traversal paragraph names `parent-child` rules as a consumer of chain relationships. State that the hierarchy findings read the edge table's `traversal = "chain"` rows.

## Out of scope

- `MissingParentLink`, `MissingRelation` and `ParentLinkRule` itself (`:526-606`) -> ITERATION-384, where the declarations they read stop loading. Leaving them on the old derivation for one slice is deliberate: they are the only checkers whose *findings* die with the rules table, so moving them here would change `validate` output twice for the same reason.
- `require_parent_status` and the `create` gate -> ITERATION-381. The gate reads `config.rules` directly, not `hierarchy_from_config`.
- Refusing `[[rules]]` at load, and deleting `ValidationRule` -> ITERATION-384, ITERATION-385. After this slice the repo declares no rules, but the shape still loads for everyone else.
- Per-edge `traversal` parsing, the chain predicate, and `child_types_for` -> STORY-257. This slice consumes them; it does not build them.

## Principles / conventions

`lazyspec convention` and the dictums it lists. Dictum 3: the predicate and the checkers are engine-side. Dictum 6: one edge index, not one per consumer -- `StatusConsistencyRule`, `child_types_for` and the chain walk all want the same reverse lookup.

## Verification

`cargo run -- validate --json` on this repo produces the same finding set at the end of the slice as it does at the start, with `git diff .lazyspec.toml` showing `[[rules]]` replaced by `[[edges]]`. The five findings this slice moves are warnings and errors this repo actually carries, so a silent drop shows up here rather than only in fixtures.
