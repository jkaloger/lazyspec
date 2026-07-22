---
title: Engine fuzzy matcher and ranked search
type: iteration
status: complete
author: jkaloger
date: 2026-07-20
tags: []
related:
- implements: STORY-129
- blocks: ITERATION-341
- blocks: ITERATION-342
---

## Objective

Engine `search()` returns fuzzy-subsequence-matched, score-ranked results across title/tags/path/body; add `score` to `SearchResult`.

## Satisfies

STORY-129 — all ACs (subsequence match, score-desc order, deterministic tie-break, score-floor exclusion, body-field match, one result per doc = best field, `score` exposed).

## Context

- Story + ACs: STORY-129
- Design + decisions (nucleo, score floor, tie-break by path, no cap): RFC-043 §Sketch/§Decisions
- Body-in-matcher + full `nucleo` crate rationale: ADR-013
- Touch:
  - `Cargo.toml` — add `nucleo` (full crate, not `nucleo-matcher`; per ADR-013)
  - `src/engine/store.rs` — `SearchResult` (571): add `score`; `Store::search` (349): rewrite substring `.contains()` path → nucleo scoring; sort (396) date→score
  - `src/engine/store.rs` tests mod (577)

## Tasks

1. Add `nucleo` dep to `Cargo.toml`.
2. `SearchResult` (store.rs:571): add numeric `score` field.
3. Rewrite `Store::search`: per doc, score query vs title/tags/path/body via nucleo; keep best-scoring field → `match_field` + `snippet` (preserve existing body ±40-char window at 384-386); drop below score floor; one result/doc; sort score-desc, tie-break by path.
4. Test-first (store.rs tests mod): non-contiguous subsequence title match (e.g. `enfz`→`engine fuzzy`); ranking order; equal-score tie-break stable across repeated runs (by path); score-floor drops non-matcher; body-only match → `match_field == "body"`.

## Out of scope

- CLI `--json` `score` + result order → STORY-131.
- TUI filter, highlight, lazy body streaming → STORY-130.
- Match-index highlight API — TUI drives its own nucleo instance (STORY-130); not exposed here.

## Principles/conventions

- .lazyspec convention: engine layer only, no CLI/TUI dep (principle 3); nucleo per Rust ecosystem norm (principle 5); trait seams only where two uses exist (principle 6).

## Verification

- Repeated `search` runs over equal-score docs → identical order (tie-break by path).

