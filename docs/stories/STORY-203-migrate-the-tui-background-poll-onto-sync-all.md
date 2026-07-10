---
title: Migrate the TUI background poll onto sync_all
type: story
status: complete
author: jkaloger
date: 2026-07-09
tags: []
related:
- implements: RFC-057
- related-to: STORY-201
---

## Context

As a TUI user, I want the background poll to refresh through the same `sync_all` the CLI uses, so that ClickUp status colours — plus git-ref refresh and GitHub project fields — appear automatically without me running `lazyspec fetch` by hand.

Today the TUI poll (`event_loop.rs`) refreshes the ClickUp task cache but never writes `status-colors.json`, never refreshes git-ref types, and never injects project fields — so a TUI-only user's ClickUp statuses render `Color::Reset` (the drift bug behind STORY-201's TUI surface). This story migrates the poll thread onto the engine seam built in STORY-202. Closing the colour bug and gaining git-ref refresh + project-field injection are free consequences of unification, not separate features.

Supersedes the dropped ITERATION-285. Satisfies STORY-201 on the TUI surface.

## Acceptance Criteria

- **Given** the TUI running with a clickup-tasks type bound to a List with per-status colours
  **When** the background poll fires
  **Then** the poll writes the derived colours to `status-colors.json` (the deliverable of this slice) and, on the next render, ClickUp statuses show their derived hex — no manual `lazyspec fetch` required. (Rendering itself already works per ITERATION-284; this closes the gap that the *poll* never wrote the sidecar.)

- **Given** a configured git-ref type
  **When** the background poll fires
  **Then** its cache refreshes (previously skipped entirely by the poll).

- **Given** a configured github-issues type
  **When** the background poll fires
  **Then** project fields are injected into the cache (previously CLI-only).

- **Given** the edit-push path (`try_push_gh_edit`) and the `gh_issue_map_stale` reload
  **When** a poll runs sharing `GithubIssuesStore.issue_map`
  **Then** both read the one authoritative map — the poll borrows `&mut store.issue_map` into `SyncContext`, no duplicated/drifting copy.

- **Given** a per-type fetch failure during a poll
  **When** it occurs
  **Then** the poll warns-and-continues and folds each outcome's `error` + `warnings` into the existing `CacheRefresh { warnings }` channel (no event-shape change), and never aborts the poll.

- **Given** a poll whose outcomes carry derived lifecycles
  **When** it completes
  **Then** `.lazyspec.toml` is **not** rewritten (a background poll must not mutate tracked config; lifecycle persist stays CLI-only).

## Scope

### In Scope

- Migrate the poll thread onto `sync_all`: lock `shared_gh_store`, borrow `&mut store.issue_map` into a per-poll `SyncContext`, build per-poll ClickUp maps, build/reuse `Syncers`.
- Save through the borrow after `sync_all`: `GithubIssuesStore.issue_map` (owned field, mutated in place) + per-poll `task_map` + **`status_colors`** (the fix).
- Fold every `SyncOutcome` `error` + `warnings` into the existing `CacheRefresh { warnings }` channel (no new event field).
- Remove `event_loop::refresh_clickup_cache` and the bespoke poll-thread fetch loop.

### Out of Scope

- The engine seam itself (built in STORY-202; this story only wires the poll to it).
- Lifecycle persist in the poll (deliberate asymmetry — poll never rewrites `.lazyspec.toml`).
- Any renderer change (foreground status colour rendering already correct per ITERATION-284).
- Poll-latency optimization for the new project-field GraphQL cost (TTL / diff-only injection is a bounded follow-up, not this slice).

