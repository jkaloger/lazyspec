---
title: Sync inherits remote issue/milestone open-closed into lifecycle
type: iteration
status: accepted
author: unknown
date: 2026-07-18
tags: []
related:
- implements: STORY-223
- related-to: BUG-008
- blocks: ITERATION-319
---

## Objective

Sync maps remote issue/milestone open/closed → type first-active/terminal lifecycle state (read direction). Fixes BUG-008 read half.

## Satisfies

STORY-223 AC1, AC3, AC4. AC2 (write-through close) deferred → sibling iteration (see Out of scope).

## Context

- Layer: engine only (src/engine) — sync + github-backed status derivation.
- Bug: github-backed docs carry lazyspec-local lifecycle disconnected from remote; closed issue/milestone stays at local status (BUG-008). Remote is source of truth.
- Mapping (AC1, default open/closed binary this slice): remote `open` → type first ACTIVE lifecycle state; remote `closed` → type TERMINAL lifecycle state. Custom lifecycles use their own first-active/terminal.
- Builds on ITERATION-317 birth-state seeding (same status-model code) → blocked-by it.

## Tasks

1. Test-first: engine test — synced issue `open` → doc status == first active state; issue `closed` → terminal state. Milestone open/closed likewise. Custom-lifecycle type uses its own states. fs/git-ref types untouched (AC4).
2. Define remote-state → lifecycle-state mapping per github-backed type (first-active / terminal derivation from `TypeDef.lifecycle.states`).
3. Apply mapping on sync read path for issues AND milestones → remote transition (issue closed on GitHub) surfaces after sync/TUI poll w/o local edits (AC3).
4. Guard fs/git-ref stores: no lifecycle override (AC4).

## Out of scope

- AC2 write-through: lazyspec transition into terminal closing remote → sibling iteration (blocked-by this).
- Per-state custom mapping config (open/closed binary only).
- ClickUp lifecycle inheritance.

## Principles/conventions

- CLAUDE.md: engine no I/O assumptions; reuse ITERATION-317 status-model, one rework for BUG-007+BUG-008. TUI/CLI sync consumers must reflect inherited status.

## Verification

Close issue on GitHub → `sync` → doc status == terminal state. fs/git-ref unaffected. `cargo test`.
