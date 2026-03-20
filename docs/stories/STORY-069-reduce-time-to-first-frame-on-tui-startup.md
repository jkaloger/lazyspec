---
title: Reduce time-to-first-frame on TUI startup
type: story
status: draft
author: jkaloger
date: 2026-03-19
tags: []
related:
- related-to: docs/audits/AUDIT-005-tui-startup-performance.md
---


## Context

AUDIT-005 identified that `refresh_validation()` runs synchronously after `App::new()` but before the event loop starts, blocking the first frame render. Additionally, `rebuild_search_index()` is called twice during init with no document changes between calls. Together these add unnecessary latency to time-to-first-frame.

## Acceptance Criteria

- **Given** the TUI is starting up
  **When** the alternate screen is entered
  **Then** the first frame renders before validation completes (validation runs after or in the background)

- **Given** `App::new()` has been called
  **When** `refresh_validation()` is called immediately after
  **Then** `rebuild_search_index()` is only invoked once across both calls, not twice

- **Given** validation is deferred
  **When** validation results arrive
  **Then** the validation indicators in the UI update without requiring user interaction

## Scope

### In Scope

- Deferring `validate_full()` to after the first frame render or to a background thread (F3)
- Removing the duplicate `rebuild_search_index()` call during init (F5)

### Out of Scope

- Changes to `Store::load` (F2, not selected for action)
- Parallelizing file I/O with rayon
- Async/tokio migration
