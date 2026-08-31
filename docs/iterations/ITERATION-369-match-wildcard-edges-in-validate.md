---
title: Match wildcard edges in validate
type: iteration
status: in-progress
author: Jack Kaloger
date: 2026-08-31
tags: []
related:
- implements: STORY-256
- blocks: ITERATION-371
---

## Objective

A wildcard row matches during `validate`: `via = "*"` matches any relationship, `to = "*"` any target type, `from = "*"` any source type — so `via = "*"`, `to = "*"`, `required = "error"` reports a document of `from`'s type that carries no relation at all.

## Satisfies

STORY-256 AC1, AC6. AC1 and AC6 are the same predicate read on different positions — one matcher, one test table — so they are not separable into two slices. AC2, AC3, AC4 deferred; AC5 landed in the preceding iteration.

## Context

- Story + ACs: STORY-256
- Matching semantics per position: ADR-031 §Decision
- `via = "*"`, `to = "*"`, `required = "error"` is the shape `relation-existence` translates to: RFC-067 §Design
- Touch:
  - `src/engine/config.rs` — the selectors added in the preceding iteration gain the match predicate
  - `src/engine/validation.rs` — `RequiredEdgeRule` compares `meta.doc_type` against `edge.from`, `r.rel_type` against `edge.via`, and the resolved target's type against `edge.to` by string equality; all three become selector matches. `ValidationIssue::UnsatisfiedEdge` and its `Display` arm print `to_types.join(", ")` and the bare `via`
  - `README.md` §`[[edges]]`
- The existing `relation-existence` checker stays put — AC6 makes edges able to express the same finding, it does not retire the rule (STORY-259).
- Per ITERATION-367, `ValidationIssue` reaches the TUI and web view only through `Display`; re-confirm no consumer matches variants before assuming a message-only change is enough.

## Tasks

1. Test-first on the predicate: a `from = "*"`, `to = "*"`, `via = "related-to"` row matches the concrete triple for any two document types linked `related-to`, and does not match a triple whose relationship is something else (AC1). The predicate is the only observable surface for AC1 in this story: the row carries no `required` (and could not, per AC4), and traversal does not arrive until STORY-257.
2. Add that predicate to `EdgeDef`: given a concrete source type, relationship name and target type, does this row match. `Any` matches anything; `Types` matches membership.
3. Rewrite `RequiredEdgeRule`'s three string comparisons to go through the predicate. Keep the requirement that the relation resolves to a document present in the store: `to = "*"` means "any document", not "any string in the `related` list" — a dangling target already has its own finding.
4. Test AC6: `from = "iteration"`, `via = "*"`, `to = "*"`, `required = "error"`; a document of that type with an empty `related` reports the finding, and one carrying any single relation to any resolvable document does not.
5. Fix the `Display` arm for wildcard positions. `needs "*" to one of: *` is not a sentence; a wildcard `via` reads as any relationship and a wildcard `to` as a document of any type. Cover both in the test.
6. README: add the `relation-existence`-equivalent row to the `[[edges]]` reference and state what each wildcard position matches.

## Out of scope

- Two rows matching the same edge — most-specific-wins (AC2) and equal-specificity contradiction (AC3) — and `required` on a wildcard `from` (AC4). Until those land, overlapping rows each fire independently; do not paper over that here.
- `traversal` and `context` → STORY-257. `fix --config` → STORY-258.

## Principles / conventions

`lazyspec convention` and the dictums it lists. Dictum 3: the predicate and the checker are engine-side; the CLI only formats the message.

## Verification

`cargo run -- validate --json` on this repo is unchanged — this repo declares no edges. On a scratch project, a single `via = "*"`, `to = "*"`, `required = "error"` row reproduces the finding its `relation-existence` rule produces for the same type.
