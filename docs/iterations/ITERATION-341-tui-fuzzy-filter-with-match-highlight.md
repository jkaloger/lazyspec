---
title: TUI fuzzy filter with match highlight
type: iteration
status: complete
author: jkaloger
date: 2026-07-20
tags: []
related:
- implements: STORY-130
---

## Objective

TUI live filter uses fuzzy scorer: subsequence match, score-desc rows, body coverage via lazy in-memory-cached body reads, matched-char highlight.

## Satisfies

STORY-130 — all ACs (subsequence match kept, score-desc rows, live update, body-only match appears, matched chars highlighted, empty on no-match via score floor).

## Context

- Story + ACs: STORY-130
- Design: RFC-043 §Sketch; body coverage + lazy in-memory body cache: ADR-013 (see its "Deviation from the original streaming design" — the nucleo injector/DiskCache were not built; TUI routes through `Store::search`)
- Engine scorer + score floor: STORY-129 (blocking)
- Touch:
  - `src/tui/state/app.rs` — `SearchEntry` (243), `rebuild_search_index` (1823, metadata inject), `update_search` (2230): replace `.contains()` (2241) + alpha `sort()` (2244)
  - `src/tui/views/overlays.rs` — `draw_search_overlay` (945), rows (968): highlight
  - `src/tui/infra/event_loop.rs` — `rebuild_search_index` calls (485/555/567/592); file-watch invalidation

## Tasks

1. Replace `update_search` substring path with a call to the shared engine `Store::search`; drop the `SearchEntry`/`search_index`/`rebuild_search_index` substring machinery (metadata read live from `store.docs`).
2. Results arrive score-desc from the engine (drop the alpha `results.sort()`); live update as query changes.
3. Body coverage via lazy in-memory cache (ADR-013): `Store::search` reads each body on first query and memoizes it in an in-memory `body_cache`; `reload_file`/`remove_file` invalidate on file-watch. (No background streaming/injector; no `DiskCache` — see ADR-013 deviation note.)
4. Highlight matched chars in `draw_search_overlay` rows using engine `match_indices` (same nucleo config as `search`).
5. Tests: subsequence title match retained; score-desc order; body-only match appears; no-match → empty (score floor); highlight indices present on matched row.

## Out of scope

- Engine matcher/scoring/floor → STORY-129.
- CLI ranking + `score` → STORY-131.
- Link-editor fuzzy filter.

## Principles/conventions

- .lazyspec convention: TUI depends on engine only, never CLI (principle 3); engine owns matching; fast startup preserved — `body_cache` empty at boot, bodies read lazily on first search (ADR-013).

## Verification

- Startup latency unchanged (metadata-only at boot; no body reads until first search).

