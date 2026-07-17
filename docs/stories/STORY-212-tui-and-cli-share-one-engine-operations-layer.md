---
title: TUI and CLI share one engine operations layer
type: story
status: complete
author: agent
date: 2026-07-16
tags: []
related:
- related-to: AUDIT-018
---

## Value

As a lazyspec developer, document operations live in one engine layer: TUI and CLI call the same code, and wiring a new backend touches one place (CONVENTION principle 3).

## Acceptance Criteria

- AC1: no `crate::cli::` import remains anywhere under `src/tui/`. (AUDIT-018 F2)
- AC2: create/link/delete/update/fix operation functions live in an `engine::ops` (or equivalent) module; CLI and TUI are thin callers.
- AC3: backend store construction goes through `build_registry`/a shared helper — the ClickUp token+store block exists exactly once. (AUDIT-018 F7)
- AC4: behavior unchanged: existing CLI/TUI integration tests pass unmodified (mechanical hoist, no semantics change).

## Out of scope

Engine stderr warnings routing to TUI warnings panel (AUDIT-018 F3) — needs STORY-163's panel plumbing; separate story.

