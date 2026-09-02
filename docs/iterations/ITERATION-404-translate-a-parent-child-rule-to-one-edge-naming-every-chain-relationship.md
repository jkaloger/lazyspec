---
title: Translate a parent-child rule to one edge naming every chain relationship
type: iteration
status: accepted
author: Jack Kaloger
date: 2026-09-02
tags: []
related:
- implements: STORY-258
- blocks: ITERATION-380
---

## Objective

A `parent-child` rule translates to one edge whose `via` names every chain-marked relationship, so the migration preserves the checker's disjunction instead of turning it into a conjunction.

## Satisfies

STORY-258 AC2 (restated by ADR-032's second amendment) and AC5, which closes the story.

## Context

- Story + ACs: STORY-258
- Decision, second amendment, and the measurement behind it: ADR-032 §Decision -- one row per chain relationship gives two demands of equal specificity with disjoint `via`, neither displaces the other in `undisplaced_demands` (`src/engine/validation.rs:641`), and the document ends up needing both links
- The set-valued `via` this depends on: ITERATION-403
- Touch:
  - `src/engine/ops/fix/config.rs:158` -- `edges_from_rule(rule, chain)` returns a `Vec<EdgeDef>` because a parent-child rule fanned out. It now returns one row per rule. The `-via-<name>` suffix on the emitted edge name goes with the fan-out; the rule's own name is the edge's name.
  - `tests/integration/cli_fix_config_test.rs` -- `the_finding_set_survives_the_migration` is committed `#[ignore]`d naming this exact divergence. Removing the attribute is the acceptance test.
  - `.lazyspec.toml` -- this repository marks both `implements` and `targets` chain, so it is the fixture that exercises the case
- A rule matching zero chain relationships still has no row to emit; keep whatever this does today and cover it.

## Tasks

1. Remove the `#[ignore]` and its comment from `the_finding_set_survives_the_migration` and watch it fail for the stated reason. It is the red.
2. Test-first, in `ops/fix/config.rs`: a `parent-child` rule against a config marking two relationships chain emits exactly one edge, named for the rule, whose `via` names both.
3. Make it pass. Drop the per-relationship name suffix.
4. Migrate this repository's own `.lazyspec.toml` with the dev binary and confirm `validate` reports the same finding set before and after -- the failure ADR-032 records is on this config, so this is the real check, not a fixture.
5. Reconcile the migration's own dry-run text and JSON with the new row count if either counts or names the rows it would add.

## Out of scope

- `store.chain_relationships` (`src/engine/store.rs:122`) is still derived from `relationships[].traversal`, which the migration deletes, so `RejectedParent`, `SupersededParent`, `OrphanedAcceptance` and `StatusConsistencyRule` (`validation.rs:486,521,583,752`) go silent on a migrated config. That is a second route by which the finding set is not identical, it is larger than this slice, and ITERATION-380 is where the surviving hierarchy findings get re-derived from the edge table. Do not start it here; confirm it is still ITERATION-380's and say so in the report if it is not.
- Narrowing the wildcard rows the migration emits for relationship traversal. ADR-032 §Consequences leaves that to the author.

## Principles/conventions

`cargo run --quiet -- convention`.

## Verification

`cargo run --quiet -- validate --json` on this repository, before and after a `fix --config` applied to a copy, produces the same finding set -- and no story is warned for using `targets` rather than `implements`.
