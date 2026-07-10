---
title: Assert ClickUp custom task type on clickup-tasks types
type: story
status: in-progress
author: unknown
date: 2026-07-10
tags: []
related:
- implements: RFC-056
---

## Context

`clickup-tasks` document types bind to a ClickUp List but cannot constrain *which* ClickUp custom task type their tasks are. `github-issues` types already carry a `github_issue_type` field for this classification (schema-only today, per RFC-056 / config.rs). ClickUp exposes the custom task type as the task's `custom_item_id` (integer) on read (`src/engine/clickup.rs:239`), but there is no config binding, no read filter, and no create-payload field for it. This story adds a fully-wired equivalent.

## User Story

As a lazyspec user binding a document type to a ClickUp List, I want to assert a specific ClickUp custom task type for that type, so that the List materializes only tasks of that type and new tasks are created with it.

## Acceptance Criteria

- `TypeDef` gains a `clickup_task_type` field that round-trips through `.lazyspec.toml` and appears in `config --json`.
- The field is resolved to ClickUp's numeric `custom_item_id` (accept a numeric id directly; name-to-id resolution via the ClickUp API is a stretch decision to nail down at iteration time).
- Read filter: a `clickup-tasks` type with `clickup_task_type` set materializes only tasks whose `custom_item_id` matches; non-matching tasks in the bound List are skipped during sync/`task_list`.
- Write on create: `create` sends the asserted `custom_item_id` in the `TaskCreate` POST body, so new ClickUp tasks are stamped with the type.
- Validation: `clickup_task_type` is only valid on `store = clickup-tasks`; any other store errors clearly (mirror the existing store/field guards in config.rs).
- Config CLI (`config add-type` and relevant mutators) accepts the new field; README store docs updated.
- Tests cover: config round-trip, read filter (match + skip), create-payload stamping, and the store-mismatch validation error.
- Parity note: unlike `github_issue_type` (schema-only), this field is fully wired end to end. TUI, web view, and CLI stay consistent per project rule.

## Non-Goals

- Retagging or migrating existing ClickUp tasks.
- Changing `github_issue_type` behavior.
