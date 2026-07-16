---
title: Hoist doc ops into engine ops layer and dedupe store wiring
type: iteration
status: accepted
author: agent
date: 2026-07-16
tags: []
related:
- implements: STORY-212
---

## Objective

Hoist doc ops (create/link/delete/update/fix) from CLI into engine ops layer; TUI stops importing `crate::cli`; store wiring deduped through registry.

## Satisfies

STORY-212 AC1, AC2, AC3, AC4.

## Context

- Blocked by: lease removal (main.rs gates), mutation fixes + --json iterations (same files).
- Findings: AUDIT-018 F2, F7
- TUI import sites: `src/tui/state/app.rs:2450,2503,2528,2554,2613,2894,3173`, `src/tui/infra/event_loop.rs:902`
- Dup store wiring: `src/cli/create.rs:132,157,181,324`, `src/cli/update.rs:94-118`, `src/cli/delete.rs:33-50`, `src/cli/link.rs:660`, `src/tui/infra/event_loop.rs:186,596`; registry: `src/engine/store_dispatch.rs:2289` `build_registry`
- Convention: CONVENTION principle 3, 6; DICTUM-003 (module structure); web layer header `src/web/routes.rs:3` = target discipline

## Tasks

1. New `engine::ops` module; move op fns (no clap types in signatures — mechanical).
2. Rewire CLI + TUI callers; delete TUI `crate::cli::` imports.
3. Shared write-store helper (ClickUp token block ×1) in store_dispatch; route create.rs GitHub branches through registry.
4. `cargo test` — existing integration tests pass unmodified (AC4).

## Out of scope

Engine stderr/warnings routing (STORY-212 out-of-scope). Behavior changes. `cli::fix::run_human` human-formatting stays CLI-side; move only the op core.

## Verification

`grep -rn "crate::cli" src/tui/` → zero. ClickUp token-load block appears once in src/.

