---
title: Unify fetch behind an engine sync_all and migrate the CLI
type: story
status: in-progress
author: jkaloger
date: 2026-07-09
tags: []
related:
- implements: RFC-057
- blocks: STORY-203
---

## Context

As someone running `lazyspec fetch` — in a script or by hand — I want every configured type refreshed by one engine-level orchestrator, so that a single failing type surfaces its own error and fails the run without silently aborting the types behind it.

Fetch is implemented twice today (CLI `src/cli/fetch.rs`, TUI poll `src/tui/infra/event_loop.rs`) and has drifted (RFC-057, Motivation). This story introduces the engine seam — `TypeSync` + borrowed `SyncContext` + `Syncers` + `sync_all` dispatching over `match StoreBackend` — and migrates the CLI wholesale onto it. All four syncers ship here so the dispatch match is exhaustive from birth; git-ref is included, not deferred.

Behaviour-preserving for the CLI **except** the recorded error-model change (continue-then-exit-non-zero instead of `?`-abort-on-first-failure) and the `--json` `error`-field addition. This is the walking skeleton: the thinnest end-to-end slice that runs the whole fetch stack through the new seam on one surface (CLI). The TUI poll migration is STORY-203.

## Acceptance Criteria

- **Given** configured github-milestones, github-issues, git-ref, and clickup-tasks types
  **When** `lazyspec fetch` runs
  **Then** every type's cache refreshes through `sync_all`, with all milestone types fetched before all issue types.

- **Given** a milestone type and an issue type with a cross relation
  **When** `sync_all` runs
  **Then** the issue resolves its milestone relation correctly (ordering constraint holds).

- **Given** every configured type fetches successfully
  **When** `lazyspec fetch` runs
  **Then** every cache is persisted and the process exits zero.

- **Given** one configured type whose fetch fails
  **When** `lazyspec fetch` runs
  **Then** the remaining types still refresh, the failing type's error prints, everything that succeeded is persisted, and the process exits non-zero.

- **Given** no ClickUp token, or an unresolvable GitHub repo
  **When** `lazyspec fetch` runs
  **Then** it hard-errors while *constructing* `Syncers` — before any type is synced — distinct from a per-type sync failure (this stays a `?`-abort, not a `SyncOutcome.error`).

- **Given** a github-issues type whose project-field GraphQL injection fails
  **When** `lazyspec fetch` runs
  **Then** the cached doc keeps its other fields, a `warning` (not `error`) is recorded on the outcome, and — absent any real `error` — the process exits **zero** (a warning is not a failure).

- **Given** `--json` and one failed type
  **When** `lazyspec fetch` runs
  **Then** that entry gains an `"error": "<msg>"` field, successful entries keep the exact `{type,fetched,new,removed}` shape, and the process exits non-zero.

- **Given** a github-issues type
  **When** `lazyspec fetch` runs
  **Then** project fields are injected into the cache (the previously CLI-only `inject_project_fields_into_cache` logic, now folded into `GhIssueSync`).

- **Given** a clickup-tasks type bound to a List with per-status colours
  **When** `lazyspec fetch` runs
  **Then** status colours are captured to the gitignored `status-colors.json`, and the derived lifecycles are persisted to `.lazyspec.toml`.

- **Given** a filesystem type or a github-projects type
  **When** `lazyspec fetch` runs
  **Then** it is an explicit skip arm in the dispatch (filesystem has no remote; github-projects fields are pulled within `GhIssueSync`, not as a top-level backend).

- **Given** `--type T`
  **When** `lazyspec fetch` runs
  **Then** only type `T` refreshes.

- **Given** no fetchable types configured
  **When** `lazyspec fetch` runs
  **Then** the existing "no fetchable types" message prints.

- **Given** a configured backend with no syncer in `Syncers`
  **When** `sync_all` runs
  **Then** that type yields an `error`-bearing `SyncOutcome` rather than a panic.

## Scope

### In Scope

- New `src/engine/sync.rs`: `TypeSync` static (non-`dyn`) contract, `SyncContext<'a>` / `GhMaps<'a>` / `ClickupMaps<'a>`, `SyncOutcome`, `Syncers` (per-backend `Option` fields), `sync_all` with never-abort error model, `match StoreBackend` dispatch (incl. Filesystem / GithubProjects skip arms), milestones-before-issues ordering.
- Four syncers: `GhMilestoneSync`, `GhIssueSync` (project-field injection folded in), `GitRefSync`, `ClickupSync`.
- Relocate `fetch_git_ref_type` from `fetch.rs` into `GitRefSync` (engine); return `SyncOutcome`.
- Migrate `fetch::run` off its bespoke store-filtered loops onto `sync_all`; replace CLI-private `TypeSummary` with `SyncOutcome` throughout.
- CLI-side save after `sync_all`: run-local `issue_map` + `task_map` + `status_colors`, then `persist_clickup_lifecycles` from the outcomes.
- Token-absent / repo-unresolvable stay hard errors raised while *building* `Syncers` (before `sync_all`), not folded into `SyncOutcome.error`.

### Out of Scope

- TUI poll migration (STORY-203).
- Any renderer change (TUI/CLI/web status colour output stays as ITERATION-284 shipped it).
- New fetch capability beyond closing drift.
- Milestone→issue dependency guard. `sync_all` does not skip issue fetch on milestone failure: on a milestone-fetch failure the CLI now proceeds into issue fetch against a stale milestone map, silently dropping `targets` relations. This latent bug already exists in the TUI poll; the CLI inherits it here. Recorded so it is not later read as a new drift bug — a future guard can skip dependent stages on upstream failure.
- Daemon / generic job system.

