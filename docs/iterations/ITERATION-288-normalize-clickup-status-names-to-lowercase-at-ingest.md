---
title: Normalize ClickUp status names to lowercase at ingest
type: iteration
status: complete
author: unknown
date: 2026-07-09
tags: []
related:
- implements: STORY-201
---

## Objective

Lowercase ClickUp status names at ingest → lifecycle states, colour-map keys match task-payload casing → "Closed" colour resolves.

## Satisfies

STORY-201 (status colour resolution, closed-status gap).

## Context

- Story: STORY-201. Prior work: ITERATION-283 (resolver), ITERATION-277 (lifecycle from List statuses).
- Bug: ClickUp `list_statuses` return display casing ("Closed"); task payload `status.status` lowercase ("closed"). `derive_lifecycle` + `derive_status_colors` store verbatim → `StatusColors::get` (case-sensitive) miss → TUI fallback miss → white not green.
- Touch: `src/engine/clickup_cache.rs` (`derive_lifecycle`, `derive_status_colors`).
- Convention: docs/convention (engine layer, no I/O assumptions).

## Tasks

1. Test-first: mixed-case `ClickupStatus` input ("Closed") → `derive_lifecycle` states lowercase, `derive_status_colors` keys lowercase.
2. Lowercase `s.status` in both fns.
3. `cargo test`. Manual: sync → `.lazyspec.toml` lifecycle + `.lazyspec/status-colors.json` all-lowercase, closed task green in TUI.

## Out of scope

- Case-insensitive `StatusColors::get` (normalization at ingest suffice).
- TUI fallback palette change (`colors.rs` match arms).
- Migration of stale config/json — self-heal next sync.

## Verification

Closed ClickUp task render green (#008844) in TUI list after sync.

