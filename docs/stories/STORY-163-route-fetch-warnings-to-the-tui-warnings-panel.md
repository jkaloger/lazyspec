---
title: "Route fetch warnings to the TUI warnings panel"
type: story
status: draft
author: "jkaloger"
date: 2026-06-26
tags: []
related:
- implements: RFC-050
---
## Context

`IssueCache::fetch_all` (and the schema-snapshot refresh it calls) emit operational warnings with `eprintln!` at three sites in `src/engine/issue_cache.rs`: a cache-write failure (`:189`), the exact-count truncation notice (`:277`), and the schema-snapshot / gh-auth warning (`:327`, e.g. "could not refresh gh schema snapshot ... projects need `gh auth refresh -s project`"). Writing to stderr violates the layering dictum: the engine is supposed to make no I/O assumption about its caller, yet `eprintln!` hard-codes one.

The three callers of `fetch_all` are `cli/fetch.rs`, `cli/setup.rs`, and the TUI background poll thread (`src/tui/infra/event_loop.rs:540`). For the CLI, stderr is correct. For the TUI the terminal is in raw mode under the alternate screen, so raw stderr bytes paint directly onto the live frame at the cursor position. The observed corruption ("synced 4warning: could not refresh ..." bleeding across the type panel header) is exactly this: an engine-level `eprintln!` overwriting ratatui's managed buffer.

`RefreshResult` (the TTL-based `refresh_stale` path) already carries `warnings: Vec<RefreshWarning>` and its API-failure path populates it; `FetchResult` has no such field and the three `eprintln!` sites bypass the pattern entirely. The poll thread compounds it by discarding the returned `FetchResult` entirely (it only handles the `Err` arm with its own `eprintln!` at `:549`) and firing a payload-less `AppEvent::CacheRefresh`. The TUI already owns a warnings surface (`App.validation_warnings`, rendered in the warnings panel), so the fix is to make the engine return warnings and let each caller decide where they go.

This slice depends on STORY-155 only insofar as the schema-snapshot warning originates there; it changes no native-binding behaviour.

## Acceptance Criteria

- **Given** a `fetch_all` run where a cache write fails, the fetched count equals the page limit, or the schema-snapshot refresh fails
  **When** the fetch completes
  **Then** each condition appears as a `RefreshWarning` in the returned `FetchResult.warnings` and no `eprintln!` is issued from `issue_cache.rs`.

- **Given** the CLI `fetch` or `setup` command
  **When** `fetch_all` returns warnings
  **Then** the command prints each warning to stderr (current behaviour preserved), and `--json` output is unaffected.

- **Given** the TUI background poll thread completing a refresh that produced warnings (including a per-type fetch error previously logged at `event_loop.rs:549`)
  **When** the thread signals the main loop
  **Then** the warnings are carried on the `AppEvent` payload, appended to `App.validation_warnings`, and shown in the warnings panel; no bytes are written to stderr while the alternate screen is active.

- **Given** a poll refresh that produced no warnings
  **When** the main loop handles the event
  **Then** the warnings panel is unchanged and no spurious entries are added.

## Scope

### In Scope

- Replace the three `eprintln!` sites in `src/engine/issue_cache.rs` with pushes into `FetchResult.warnings` (the cache-write failure, the truncation notice, the schema-snapshot warning).
- Print collected warnings to stderr in `cli/fetch.rs` and `cli/setup.rs`.
- Extend the `AppEvent::CacheRefresh` payload (or add a sibling variant) to carry `Vec<String>`; accumulate warnings and the per-type fetch error in the poll thread, drop `event_loop.rs:549`'s `eprintln!`, and append into `App.validation_warnings` in the event handler.
- Engine unit coverage that each warning condition lands in `FetchResult.warnings`; a TUI-level assertion that the poll path routes warnings into `validation_warnings`.

### Out of Scope

- Changing what the schema-snapshot refresh warns about, or fixing the org-vs-user resolution that triggers it (STORY-165).
- De-duplication, severity levels, or expiry of warnings in the panel.
- Reworking the warnings panel UI or the `validation_warnings` data model beyond appending fetch warnings.
