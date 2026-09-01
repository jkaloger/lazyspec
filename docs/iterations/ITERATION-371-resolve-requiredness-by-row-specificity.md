---
title: Resolve requiredness by row specificity
type: iteration
status: complete
author: Jack Kaloger
date: 2026-08-31
tags: []
related:
- implements: STORY-256
---

## Objective

When a wildcard row and a more specific row both match one edge, `validate` takes requiredness from the more specific row — one finding at that row's severity, not one per matching row.

## Satisfies

STORY-256 AC2. AC1, AC3, AC4, AC5, AC6 landed in the preceding iterations; this closes the story.

AC2 reads "requiredness **and gating** come from the concrete row". There is no gating left to resolve: ADR-033 abandoned status-conditioned `create` gating rather than relocating it to the edge table, and `require_to_status` was reverted on this branch (commit 40b91f3). AC2 therefore reduces to requiredness, and nothing in this slice touches `create`.

## Context

- Story + ACs: STORY-256
- "Requiredness takes the most specific row", and the accepted coarseness of specificity-by-count: ADR-031 §Decision, §Consequences
- No edge condition refuses a command; every unsatisfied edge is a finding: RFC-067 §Design
- Gating's absence: ADR-033
- Touch:
  - `src/engine/validation.rs` — `RequiredEdgeRule` currently loops rows independently, so two overlapping rows emit two findings for one document
  - `src/engine/config.rs` — the specificity and overlap predicates built in the preceding iteration
  - `README.md` §`[[edges]]`

## Tasks

1. Test-first: a `from = "iteration"`, `to = "*"`, `via = "implements"`, `required = "warning"` row alongside a `from = "iteration"`, `to = ["story"]`, `via = "implements"`, `required = "error"` row produces exactly one finding, at `error`, for an iteration with no `implements` relation.
2. Test-first: a wildcard row demanding what a more specific overlapping row waives (`required` absent on the specific row) produces no finding at all — waiving is a resolution outcome, not a special case.
3. Implement resolution in the engine: before checking, reduce each document's applicable rows to those not overlapped by a strictly more specific row, and read `required` from what survives. The load-time checks from the preceding iteration guarantee no equal-specificity ambiguity reaches here, so resolution never has to break a tie.
4. Test that two rows which do **not** overlap both still fire — resolution must not silently swallow independent findings.
5. README: state that requiredness comes from the most specific matching row, with the ordering already documented in the preceding slice, and that resolution applies to requiredness only (ADR-033 — nothing gates a command).

## Out of scope

- Traversal composition across matching rows (ADR-031: traversal composes rather than resolving by specificity) → STORY-257.
- Finer specificity than concrete-position count → explicitly rejected by ADR-031 §Consequences; do not invent a tiebreak here.
- Retiring `[[rules]]` now that edges can express both rule shapes → STORY-259.

## Principles / conventions

`lazyspec convention` and the dictums it lists. Dictum 3: resolution is engine logic; `validate --json` and the TUI both consume the resolved result rather than re-deriving it.

## Verification

`cargo run -- validate --json` on this repo is unchanged. On a scratch project, ADR-031 §Consequences' starter shape — one wildcard `related-to` row, one wildcard `targets` row, concrete rows only where a constraint is wanted — loads, validates, and reproduces the findings the equivalent `[[rules]]` config produces.
