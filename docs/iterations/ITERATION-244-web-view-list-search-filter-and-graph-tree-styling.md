---
title: Web view list, search, filter, and graph tree styling
type: iteration
status: complete
author: unknown
date: 2026-06-30
tags: []
related:
- implements: STORY-182
---

<\!-- intent: one session-sized slice handed to a coding agent -->

## Objective

Style list + search + filter surfaces (tabular hairline rows, HTMX fragment row-parity) and the graph tree (`data-depth` indentation with guide lines, per-node type/status/related). CSS on Iter A tokens; reuse Iter B status treatment.

## Satisfies

STORY-182 AC6, AC7. (AC8 flat-surface, AC9 motion already enforced by Iter A base.)

## Context

- Story + AC text: STORY-182
- Component spec: RFC-053 sections "Component specifications" (List page, Search and filter fragments, Graph page), "Motion tokens" (HTMX swap)
- Tokens + base: ITERATION-242. Status swatch/label treatment: ITERATION-243 (reuse, do not redefine).
- Quality gates: taste-skill — no cards (hairline + whitespace), square-shape lock, accent only on interactive (hovered/active row, link underline, focus ring), density restraint, em-dash ban.
- Existing markup hooks: `templates/list_page.html` (`#filters` selects, search `<input>`, `<h1>` band), `templates/list_row.html` + `templates/list_fragment.html` + `templates/search_fragment.html` (`.doc-id`/`.doc-title`/`.doc-status` rows), `templates/graph_page.html` + `templates/graph_node.html` (`.graph-tree`, `data-depth`, `.graph-type`/`.graph-status`/`.graph-related`, dotted-arrow glyphs)
- Touch:
  - `static/lazyspec.css` — append list/search/filter block + graph block
  - templates above — only if a hook is missing; row markup in `list_row.html` and `search_fragment.html` MUST be identical for swap parity (AC6)
  - `tests/integration/web_serve_test.rs` — assert list row and search-fragment row markup match; graph nodes carry `data-depth`

## Tasks

1. Read list/search/graph templates; confirm `list_row.html` and `search_fragment.html` emit identical row structure (AC6 row-parity). Reconcile if drifted (shared partial or matched markup).
2. CSS list: top band with `--rule-strong`; search `<input>` `--surface-raised`, square, `--rule` border, accent focus ring, mono placeholder `--ink-faint`; `#filters` selects bare mono `--t-label`, hairline underline.
3. CSS rows: tabular `.doc-id` (mono fixed gutter) / `.doc-title` (sans) / right-aligned `.doc-status`; `divide` via `--rule` hairlines, no cards; hover wash `--accent-weak`.
4. CSS fragments: identical row rule applies to search/filter fragment rows — no layout shift on swap (AC6).
5. CSS graph: `.graph-tree` indent by `data-depth` (`--sp-4` step) with per-level `--rule` guide line; dotted-arrow glyphs `--ink-faint`. `.graph-type` mono no color; `.graph-status` micro-label + swatch (reuse Iter B); `.graph-related` accent-underlined mono ids. `.graph-empty`/`.search-empty` `--ink-faint`.
6. Tests: row-parity assertion (list vs search fragment), graph `data-depth` present, hairline-not-card class assertions.

## Out of scope

- Doc page grid/status/accent → AC3-5 (Iter B).
- Tokens/theming/fonts/base/motion → ITERATION-242.
- Graph canvas/SVG (RFC non-goal).

## Principles / conventions

- docs/convention (layering, `--json`, Rust idioms).
- taste-skill quality gates as CSS review bar (no cards, accent discipline, density).
- writing-iterations: pointer + tasks, no restated spec.

## Verification

- GET list, then `/search` fragment: swapped rows visually identical (same classes/structure).
- GET graph: nodes indent by depth with guide lines; relations accent-underlined.
- Grep list/graph CSS: no card/box-shadow; `--accent` only on hover/active/link/focus.
