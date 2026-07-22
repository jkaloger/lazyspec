---
title: "ClickUp task type filter only accepts numeric id, not name"
type: bug
status: reported
author: "unknown"
date: 2026-07-21
tags: []
related: []
---

## Context

ClickUp-backed types filter fetched tasks to a single custom task type via the config field `clickup_task_type`. That field accepts only a numeric `custom_item_id`. Users must hunt down the opaque numeric id; there is no way to name the type (e.g. `"Bug"`, `"Feature"`) in config.

## Root Cause

Numeric-only by design, name resolution never built.

- Config field: `clickup_task_type: Option<i64>` at `src/engine/config.rs:369`. Doc comment states outright: "A numeric id only -- name->id resolution is deferred."
- Filter application: `src/engine/clickup_cache.rs:61-67` filters tasks by exact numeric match `task.custom_item_id == Some(expected)`.
- Create path stamps the same numeric field onto new tasks: `src/engine/clickup_cache.rs:396,423` -> `TaskCreate.custom_item_id` (`src/engine/clickup.rs:331`).
- No lookup exists: the `ClickupClient` trait (`src/engine/clickup.rs:387-456`) has no method to list custom task types. ClickUp's real endpoint `GET /team/{team_id}/custom_item` (https://developer.clickup.com/) is never called anywhere. So there is no name->id mapping to resolve against.
- Validation ties the field to the clickup-tasks store only: `src/engine/config.rs:1158-1161`.

This is a missing feature, not a regression.

## Expected vs Actual

- **Expected:** config accepts a task type by human-readable name, e.g. `clickup_task_type = "Bug"`, and lazyspec resolves it to the numeric id against the workspace.
- **Actual:** only `clickup_task_type = 42` (numeric `custom_item_id`) is accepted; the id is hard to find in the ClickUp UI.

## Repro

1. Configure a clickup-tasks type in `.lazyspec.toml`.
2. Try `clickup_task_type = "Bug"` -> config parse/validate fails (expects int).
3. Only a raw numeric id works.

## Fix Direction

1. **Config:** widen `clickup_task_type` to accept a name or id -- untagged serde enum (`String` name | `i64` id), or a parallel `clickup_task_type_name`. Update the TOML round-trip tests (`config.rs:2813-2865`).
2. **API:** add a `list_task_types`/`custom_item` method to the `ClickupClient` trait + reqwest impl calling `GET /team/{team_id}/custom_item`, plus the fake client used in tests (`clickup.rs:981`). Needs a team/workspace id plumbed (list -> folder -> space -> team, or from config) -- not currently available.
3. **Resolution:** at fetch/create in `clickup_cache.rs` (before the filter at :61 and the stamp at :423), resolve name -> numeric `custom_item_id`, cached. Filter comparison stays numeric once resolved.
4. **Validation:** keep the store-backend check (`config.rs:1158`); add a resolution-failure error when a configured name matches no task type in the workspace.

CLI/web/TUI: filter is engine-side, shared by all three -- no surface-specific change beyond config docs (update README).

## Acceptance Criteria

- [ ] `clickup_task_type` accepts a human-readable name in `.lazyspec.toml`.
- [ ] Numeric id continues to work (back-compat).
- [ ] Name resolved to `custom_item_id` via ClickUp custom-item API, cached.
- [ ] Unresolvable name -> clear validation/fetch error naming the bad value.
- [ ] README config docs updated.
- [ ] Full check green: `cargo fmt --check`, `cargo clippy`, `cargo test`.
