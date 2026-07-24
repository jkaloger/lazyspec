---
title: Relations tab and context surface declared related list ungated
type: iteration
status: complete
author: jkaloger
date: 2026-07-24
tags: []
related:
- implements: BUG-013
---

## Objective

TUI Relations tab + CLI `context` show every declared relation from `doc.related` — no traversal-marker gate — matching web view.

## Satisfies

BUG-013 AC1–AC6 (single coupled slice: TUI + CLI parity + tests).

## Context

- Root cause + repro + fix direction (a): BUG-013 §Root Cause, §Fix Direction
- Gate being bypassed: related BFS filter src/engine/context.rs:126-129, :137-140; `store.related_relationships` src/engine/store.rs:120-125
- TUI gather to change: `relation_sections()` src/tui/state/app.rs:2322-2350 (reads only `resolve_chain` output)
- CLI gather to change: src/cli/context.rs:16,329 (prints `resolved.related` :363-373)
- Pattern to match: web builds from `doc.related` directly, src/web/render.rs:192-199
- Render side untouched: `render_relationship_sections` src/tui/views/panels.rs:1224-1334
- Convention: principle 6 — TUI + CLI = two call sites, shared engine helper justified

## Tasks

1. Test-first (state-level): doc with `related-to`, config WITHOUT `traversal = "related"` → `relation_sections()` includes it. Fails on current code.
2. Test: same relation WITH traversal marker → appears once, no duplicate against BFS-derived related.
3. Impl: engine helper merging `doc.related` (declared, all rel types) into related output, dedupe by (rel_type, id); consume from `relation_sections()`. Do NOT widen `resolve_chain` BFS itself — web layers `doc.related` separately and would double up.
4. CLI `context`: consume same helper; declared relations in text + `--json` output.
5. Regression: existing chain/children/traversal-related context tests green (AC4).

## Out of scope

- Fix direction (b) implicit-Related fallback in `store.related_relationships` — rejected, bug §Fix Direction.
- Web view — already correct.
- Config validation warning for unmarked relationships — separate concern if wanted.

## Verification

Repro from BUG-013 §Repro: strip `traversal = "related"` from `related-to` in config → relation visible in Relations tab + `context --json`. Full check: `cargo fmt --check`, `cargo clippy`, `cargo test`.
