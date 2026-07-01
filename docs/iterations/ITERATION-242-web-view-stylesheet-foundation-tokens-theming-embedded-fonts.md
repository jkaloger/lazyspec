---
title: 'Web view stylesheet foundation: tokens, theming, embedded fonts'
type: iteration
status: complete
author: unknown
date: 2026-06-30
tags: []
related:
- implements: STORY-182
- blocks: ITERATION-243
---

<\!-- intent: one session-sized slice handed to a coding agent -->

## Objective

Serve one static stylesheet + compile-time-embedded fonts from the web binary, wire into every page `<head>`. Token system, theming, flat-surface + motion base only. No per-surface component styling.

## Satisfies

STORY-182 AC1, AC2, AC8, AC9, AC10. Doc/list/graph/status component specs deferred (see Out of scope).

## Context

- Story + AC text: STORY-182 (do not restate ACs)
- Token values, theming contract, font decision: RFC-053 sections "Color tokens", "Type tokens", "Space and grid tokens", "Rule and shape tokens", "Motion tokens", "Theming"; ADR decision 1 (static CSS, no build), decision 2 (compile-time-embed fonts)
- Design quality gates: taste-skill (anti-slop, AI-tells §9). Apply only the stack-agnostic gates: accent discipline (single interactive color axis), square-shape lock (radius 0), page theme lock, em-dash ban, real-fonts-not-network, density restraint. Ignore React/Tailwind/Motion/GSAP parts — this is server-rendered Askama + static CSS per ADR-001.
- Architecture: ADR-001 single binary, no Node, no build step
- Touch:
  - `static/lazyspec.css` (new) — token `:root` block + dark override + base reset + `@font-face` + flat-surface/motion base
  - `static/fonts/` (new) — one grotesque + one mono, Latin-subset
  - `src/web/assets.rs` (new) — `include_bytes\!`/`include_str\!` the css + fonts; serve fns
  - `src/web/server.rs` — add `/static/lazyspec.css` + font routes to router
  - `src/web/mod.rs` (or wherever `web` module declared) — register `assets`
  - `templates/doc_page.html`, `templates/list_page.html`, `templates/graph_page.html`, `templates/not_found.html` — add `<link rel="stylesheet" href="/static/lazyspec.css">` + font preload to each `<head>` (only markup touch this slice)

## Tasks

1. Add font assets to `static/fonts/` (Latin subset, single weight axis each per RFC risk note). Grotesque + mono from RFC "Type tokens" fallback set.
2. `src/web/assets.rs`: `include_bytes\!` fonts + `include_str\!` css; axum handler fns returning correct `Content-Type` (`text/css`, `font/woff2`).
3. Wire routes in `server.rs`: `/static/lazyspec.css`, `/static/fonts/{...}`.
4. Author `static/lazyspec.css`:
   - `:root` token block: color (surface/ink ramp, locked vermilion `--accent`, status legend `--st-*`), type (`--font-sans`/`--font-mono` chains keeping system fallback, modular scale), space (4px base), rule/shape (`--radius: 0`), motion. Values from RFC tables.
   - `@media (prefers-color-scheme: dark)` + `[data-theme="dark"]` override: redefine color tokens only.
   - Base: reset, body background/ink from tokens, square corners global, no card/shadow primitives, `@font-face` with `font-display: swap`.
   - `@media (prefers-reduced-motion: reduce)`: collapse transitions to instant.
   - No literal colors in any rule — tokens only.
5. Add `<link>` + font preload to the 4 page `<head>` blocks.
6. Extend `tests/integration/web_serve_test.rs`: GET css route 200 + `text/css`; font route 200; doc/list/graph pages `<head>` contains the stylesheet link.

## Out of scope

- Doc page grid, status encoding, accent discipline rendering → STORY-182 AC3-5, later iteration.
- List/search/filter + graph tree component styling → AC6-7, later iteration.
- Shared `<head>` partial refactor — this slice inlines `<link>` per page; partial extraction not required.
- Manual theme toggle UI (token `[data-theme]` hook present, no control).

## Principles / conventions

- docs/convention (CONVENTION + dicta): engine/CLI/TUI layering, `--json`, traits at I/O seams, Rust idioms, indirection only at 2 uses.
- taste-skill quality gates (see Context) as the CSS review bar.
- writing-iterations: this doc is pointer + tasks, not restated spec.

## Verification

- `cargo run -- serve` then GET `/static/lazyspec.css` → 200, `text/css`; GET a doc page → `<head>` links it.
- No network font dependency: page renders on system fallback with fonts route blocked.
- Grep css for hex literals outside the `:root`/dark blocks → none.
