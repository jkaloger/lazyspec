---
title: Prove the migration preserves the finding set
type: iteration
status: accepted
author: Jack Kaloger
date: 2026-09-01
tags: []
related:
- implements: STORY-258
- blocks: ITERATION-384
---

## Objective

Prove the migration is behaviour-preserving: one repository, validated before and after `fix --config` rewrites its rules and traversal keys into edges, produces the same finding set.

## Satisfies

STORY-258 AC5. Every other AC landed in the preceding three iterations.

## Context

- Story + ACs: STORY-258
- "Migration is behaviour-preserving, so upgrading cannot break a repository's validation state": ADR-032 §Consequences
- The `targets`-satisfies-`implements` hole survives migration by design: RFC-067 §Problem.1; ADR-032 §Consequences
- Touch:
  - `tests/integration/cli_fix_config_test.rs` -- the fixture at `:48-75` builds a temp project from config text and writes no documents; this slice needs documents, so the fixture grows
  - `src/engine/validation.rs:549-605` (the two rule checkers) and `RequiredEdgeRule` (`:614+`) -- read-only; if this slice needs to change either, the translation is wrong, not the checker
- "Identical finding set" cannot mean identical strings. `MissingParentLink` and `MissingRelation` become `UnsatisfiedEdge` with a different `Display`, by construction. Equality is over (document path, severity, rule-or-edge name) -- which is the reason the first iteration on this story carries the rule's own `name` onto the translated edge.
- The known divergence, stated before the test finds it: `parent-child` is satisfied only by a relationship marked `traversal = "chain"` (`validation.rs:559-573`), while a translated `via = "*"` is satisfied by any relationship at all. A document whose only link to a parent-typed document is non-chain -- `blocks`, `supersedes` or `member-of` in this project's vocabulary -- is a finding before migration and not one after. ADR-032 §Decision claims `via = "*"` "preserves today's actual behaviour"; on this case it does not.

## Tasks

1. Grow the fixture into a temp project with documents covering every finding the old checkers produce and every near-miss: a child with no parent link, a child linked by a chain relationship to the right parent type, a child linked to the wrong type, a `relation-existence` type with an empty `related`, and one with a relation.
2. One test, set equality over (path, severity, name), asserted before and after the migration -- with both sets non-empty. Two empty sets matching proves the fixture wrote no documents, not that the migration works.
3. Add the non-chain case from Context and assert what the code actually does. If the sets diverge, stop: report the divergence against STORY-258 and ADR-032 rather than reshaping the fixture until it agrees. The available resolutions -- translate `via` to the chain relationship names, or amend ADR-032 to admit the widening -- are both decisions, not edits.
4. Assert the deliberately preserved hole: a child satisfying its `parent-child` rule through `targets` rather than `implements` yields no finding on either side. Migration must not close it; that is a human edit to `via`, and a test that assumes otherwise would block the migration for doing its job.

## Out of scope

- Changing the translation to close whatever divergence task 3 finds. This slice's job is to make it visible.
- `create` gating (`require_parent_status`): it gates a command, not a finding, so this test is blind to it by design. The preceding iteration reports its removal in the plan; STORY-259 deletes it.
- Migrating this repo's own `.lazyspec.toml`, and the strict-load error that names `fix --config` once `[[rules]]` stops loading -> STORY-259.

## Principles / conventions

`lazyspec convention` and the dictums it lists, particularly the testing desiderata: predictive (this test is the whole justification for the migration being safe), behavioural (assert findings, not which checker produced them), and deterministic (fixed fixture documents, no reliance on this repo's contents).

## Verification

Break the translation by hand -- flip a translated `via` from the wildcard to `implements` -- and confirm the test fails on the wrong-type case. A set-equality test that passes for the wrong reason is worse than no test.
