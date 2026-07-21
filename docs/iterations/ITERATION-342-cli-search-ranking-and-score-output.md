---
title: CLI search ranking and score output
type: iteration
status: complete
author: jkaloger
date: 2026-07-20
tags: []
related:
- implements: STORY-131
---

## Objective

CLI `search` surfaces per-result `score` in `--json` and orders results score-desc in both `--json` and human modes.

## Satisfies

STORY-131 — all ACs (`score` in `--json`, `--json` score-desc, human score-desc, `--type` filter keeps order, no cap, `match_field`/`snippet` unchanged).

## Context

- Story + ACs: STORY-131
- Design (no cap, engine owns floor): RFC-043 §Decisions
- Scored/ranked engine results: STORY-129 (blocking)
- Touch:
  - `src/cli/search.rs` — `json_output` (16): add `score`; `run` (29) + `run_json` (60): rely on engine score-desc, no re-sort, no cap; `filter_results` (8) preserved

## Tasks

1. `json_output` (search.rs:16): add `json["score"] = r.score`.
2. Emit results in engine order (score-desc from STORY-129) in both `run` + `run_json`; no CLI re-sort, no cap; human mode prints same order.
3. Preserve `--type` filter (`filter_results`), `match_field`, `snippet`.
4. Tests: `--json` includes numeric `score`; array score-desc; human-mode order score-desc; `--type` filters + keeps order; every match present (no cap); `match_field`/`snippet` unchanged.

## Out of scope

- Engine scorer/score/rank → STORY-129.
- TUI filter + highlight → STORY-130.

## Principles/conventions

- .lazyspec convention: every command supports `--json` (principle 2); CLI depends on engine only (principle 3); CLI formats, engine ranks — no cap in CLI.

## Verification

- `search --type <t> --json` → only that type, still score-desc.

