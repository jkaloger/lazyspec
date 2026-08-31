---
title: Enforce edges in validate
type: iteration
status: in-progress
author: Jack Kaloger
date: 2026-08-31
tags: []
related:
- implements: STORY-254
---

## Objective

`validate` enforces `[[edges]]`: a document of the edge's `from` type needs a `via` relation to a document of one of the edge's `to` types, and nothing else satisfies it.

## Satisfies

STORY-254 AC1, AC2, AC3, AC7. AC4, AC5, AC6 landed in the preceding iteration.

## Context

- Story + ACs: STORY-254
- The defect this closes — any `traversal = "chain"` relationship currently satisfies any `parent-child` rule: RFC-067 §Problem.1
- `required` over a target set means "any one member", not one per member: RFC-067 §Design
- Conventions: `lazyspec convention`
- Touch:
  - `src/engine/validation.rs` — `ParentLinkRule` (L501-587) is the shape to follow; new `ValidationIssue` variant (L10-40) and its `Display` arm (L135-165)
  - `README.md` — config reference
- `ValidationIssue` reaches the TUI and web view only through `Display`; no consumer matches its variants, so a new variant needs no TUI/web change. Confirm that still holds before assuming it.

## Tasks

1. Test-first in `src/engine/validation.rs`: a fixture config declaring `from = "iteration"`, `to = ["spike", "story", "bug"]`, `via = "implements"`, `required = "error"`, with docs covering satisfied / absent / wrong-relationship.
2. Add a `ValidationIssue` variant for an unsatisfied edge, carrying the edge `name` and the **whole** permitted target-type list. Its `Display` arm names every permitted type — "an iteration needs a story" is the wrong message when spikes and bugs are equally valid.
3. Implement the checker: for each non-ignored doc whose type is the edge's `from`, require some relation whose `rel_type == via` **and** whose resolved target's type is in `to`. `required: None` skips the doc entirely.
4. Test AC3 explicitly: `iteration --targets--> STORY-x` must not satisfy an `implements` edge. This is the regression RFC-067 §Problem.1 names; it is the point of the slice.
5. Test AC7: a config carrying both `[[rules]]` and `[[edges]]` reports findings from both, neither suppressing the other.
6. Document `[[edges]]` in the README config reference.

## Out of scope

- `require_to_status` and `create` gating → STORY-255.
- `"*"` endpoints → STORY-256. Traversal and `context` → STORY-257.
- `fix --config` migration → STORY-258. Removing `[[rules]]` or its checkers → STORY-259. Editors → STORY-260, STORY-261.

## Principles / conventions

`lazyspec convention` and the dictums it lists. Dictum 3: the checker is engine-side; the CLI only formats.

## Verification

`cargo run -- validate --json` on this repo is unchanged from before the slice, since this repo declares no edges.
