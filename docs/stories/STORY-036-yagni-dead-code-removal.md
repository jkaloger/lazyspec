---
title: YAGNI Dead Code Removal
type: story
status: accepted
author: jkaloger
date: 2026-03-06
tags:
- refactor
related:
- implements: docs/rfcs/RFC-012-architecture-review-yagni-dry-cleanup.md
---



## Context

The codebase contains speculative code that was never completed or abstractions
that serve no purpose. `ViewMode::Metrics` renders empty placeholder blocks.
`resolve_editor_from` is an indirection layer only called by its own wrapper.
`list.rs` and `search.rs` expose `run_json()` functions alongside `run()`
which already handles JSON output internally.

## Acceptance Criteria

- **Given** the `ViewMode` enum
  **When** the Metrics variant is examined
  **Then** it no longer exists (removed along with `draw_metrics_skeleton`)

- **Given** `tui/app.rs`
  **When** editor resolution is examined
  **Then** `resolve_editor_from` is inlined into `resolve_editor` (one function, not two)

- **Given** `cli/list.rs` and `cli/search.rs`
  **When** JSON output is examined
  **Then** there is a single code path for JSON output, not a separate `run_json` function

- **Given** the existing test suite
  **When** all dead code is removed
  **Then** all tests pass (updated where function signatures changed)

## Scope

### In Scope

- Removing `ViewMode::Metrics` and `draw_metrics_skeleton`
- Collapsing `resolve_editor_from` into `resolve_editor`
- Unifying `run`/`run_json` in list.rs and search.rs
- Updating affected tests

### Out of Scope

- Adding new view modes to replace Metrics
- Changing CLI output format
- Modifying other ViewMode variants
