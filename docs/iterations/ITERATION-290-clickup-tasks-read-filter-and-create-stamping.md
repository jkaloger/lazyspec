---
title: clickup-tasks read filter and create stamping
type: iteration
status: complete
author: unknown
date: 2026-07-10
tags: []
related:
- implements: STORY-205
---

## Objective

Wire the `clickup_task_type` `custom_item_id` into the ClickUp store's read filter and create payload, so a bound List materializes only matching tasks and new tasks are stamped with the type.

## Satisfies

STORY-204 AC3, AC4, AC8 (parity: fully wired end to end), and the store-layer part of AC7 (read-filter match+skip + create-stamping tests). Blocked by ITERATION-289 (needs the `clickup_task_type` config field).

## Context

- Story + ACs: STORY-204
- Design (`custom_item_id` read shape, create payload, custom-field applicability): RFC-056 §"Field mapping"
- Config field landed in ITERATION-289 (`TypeDef.clickup_task_type: Option<i64>`)
- `custom_item_id` read site: `src/engine/clickup.rs:239`
- Test seam: `ClickupClient` fake impl (per RFC-056 — mirror `GhCli` fake split)
- Conventions: docs/convention (principle 4 fakes at trait seams, principle 3 layering)
- Touch: `src/engine/clickup.rs` (read filter near `:239`, `task_create` payload), `ClickupTasksStore`

## Tasks

1. Test-first: with a fake `ClickupClient`, assert read filter keeps tasks whose `custom_item_id` matches the type's `clickup_task_type` and skips non-matching, and that create sends the asserted `custom_item_id`.
2. Read filter: when `clickup_task_type` is set, drop tasks whose `custom_item_id` differs during `task_list`/sync; when unset, materialize all (no behavior change).
3. Create stamping: when `clickup_task_type` is set, include `custom_item_id` in the `task_create` POST body.
4. Confirm TUI/web/CLI stay consistent (project rule) — no new surface, field is engine-resolved; note any view touch needed.

## Out of scope

- Retagging or migrating existing ClickUp tasks (STORY-204 Non-Goal).
- Name→`custom_item_id` resolution (numeric only, per ITERATION-289 decision).
- Any `github_issue_type` behavior change (Non-Goal).

## Principles/conventions

- docs/convention dicta; principle 4 (fake only at the `ClickupClient` seam, real impl by default).
- Layer separation (principle 3): filter/stamping live in engine; no CLI/TUI logic duplication.

## Verification

Read: a bound List with mixed `custom_item_id` values materializes only the matching subset. Create: a new task POST carries the asserted `custom_item_id`; with the field unset, read/create behavior is unchanged.

