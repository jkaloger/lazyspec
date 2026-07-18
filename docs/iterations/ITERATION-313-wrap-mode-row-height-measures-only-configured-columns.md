---
title: Wrap-mode row height measures only configured columns
type: iteration
status: complete
author: unknown
date: 2026-07-18
tags: []
related:
- implements: STORY-216
- related-to: BUG-003
---

## Objective

Wrap-mode TUI list row height counts tag/provenance wrap lines only when those columns configured. Fixes BUG-003.

## Satisfies

BUG-003 (defect under STORY-216). No new STORY-216 AC — bug in the configurable-columns machinery STORY-216 added.

## Context

- Layer: TUI only (src/tui). Engine/CLI untouched.
- Bug: `row_content_lines` (src/tui/views/panels.rs:449, called :818) measures tag+provenance wrap heights unconditionally → hidden columns inflate row lines. Non-default column configs only.
- Column set = `[tui.table] columns` (STORY-216, `GraphConfig`/table config in src/engine/config.rs). Height calc + render must consult SAME set.

## Tasks

1. Test-first: TUI unit test — `row_content_lines` with columns config excluding tags+provenance on doc w/ many tags/provenance → height == configured-columns-only, NOT inflated. Add complementary case: columns INCLUDING tags/provenance still measured.
2. Gate tag-wrap + provenance-wrap measurement in `row_content_lines` (panels.rs:449) on configured column set (built-ins + custom-attr ids, same semantics as render at :818).
3. Confirm render path + height path read one shared column list (no divergence).

## Out of scope

- Non-wrap mode (already correct).
- Column config semantics/schema (owned by STORY-216 base work).

## Principles/conventions

- CLAUDE.md: TUI depends on engine, never on CLI. TUI-only change.
- caveman comments: comment only non-obvious gating rationale.

## Verification

Config columns w/o tags/provenance + wrap mode + doc w/ many tags → row height unchanged vs a doc w/ none. `cargo test`.
