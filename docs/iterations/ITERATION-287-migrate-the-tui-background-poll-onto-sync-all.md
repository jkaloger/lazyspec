---
title: Migrate the TUI background poll onto sync_all
type: iteration
status: complete
author: jkaloger
date: 2026-07-09
tags: []
related:
- implements: STORY-203
---

## Objective

Rewire the TUI background poll onto the engine's `sync_all` so a poll refreshes every configured type identically to the CLI — capturing ClickUp status colours to `status-colors.json`, refreshing git-ref types, and injecting GitHub project fields — with no manual `lazyspec fetch`.

## Context

- Story + ACs: STORY-203 (all five ACs; see Satisfies).
- Design: RFC-057 §"Caller wiring" (TUI poll paragraph — lock `shared_gh_store`, borrow `&mut store.issue_map` into `SyncContext`, build per-poll clickup maps + `Syncers`, call `sync_all`, map outcomes into `CacheRefresh`, save through the borrow, no lifecycle persist), §"Save (no flush)" (TUI bullet — save `issue_map` + per-poll `task_map` + `status_colors`, no `persist_clickup_lifecycles`), and the two ADRs: "sidecar maps are caller-owned and borrowed into `SyncContext`" and "the background TUI poll does not persist derived lifecycles to config".
- Engine seam consumed (do NOT re-spec): STORY-202 / ITERATION-285 — `sync_all`, `SyncContext` / `GhMaps` / `ClickupMaps`, `SyncOutcome`, `Syncers`, `TypeSync`.
- Touch: `src/tui/infra/event_loop.rs` — the poll thread spawned at the `shared_gh_store.is_some() || has_clickup_types` branch (currently the GitHub arm at `store.issue_map` + the ClickUp arm calling `refresh_clickup_cache`), and `refresh_clickup_cache` itself (remove) plus its two unit tests. `try_push_gh_edit`, the `gh_issue_map_stale` reload, and `shared_gh_store` are the map-sharing constraints — read, do not change their contract.
- `AppEvent::CacheRefresh { warnings }` in `src/tui/state/app.rs` — the existing channel outcomes fold into (no event-shape change).
- Conventions/dictums: CONVENTION; DICTUM-003 (Module Structure — clients/tokens injected in the TUI layer, engine holds no I/O); DICTUM-004 (Testing — drive through the existing client fakes); DICTUM-007 (TUI Patterns).

## Satisfies

STORY-203: the ClickUp status-colours-to-`status-colors.json` AC (the deliverable of this slice); the git-ref refresh AC; the github-issues project-field injection AC; the shared-`issue_map` (borrow `&mut store.issue_map`, no drifting duplicate) AC; the per-type-failure warn-and-continue folding `error` + `warnings` into `CacheRefresh { warnings }` AC; and the `.lazyspec.toml`-not-rewritten AC (no lifecycle persist in the poll). No ACs deferred.

## Tasks

1. In the poll thread, after locking `shared_gh_store`, build a `SyncContext` per RFC-057 §"Caller wiring": borrow `&mut store.issue_map` into `GhMaps` (populate `gh` only when the github store exists), and build per-poll `task_map` + `status_colors` maps for `ClickupMaps` (populate `clickup` only when clickup-tasks types are configured). Build a `Syncers` with the real clients/tokens the two arms already construct (`GhCli`, `ClickupHttpClient` + `load_clickup_token`), reusing across the poll.
2. Replace both bespoke arms (the milestone→issue loop and the ClickUp arm) with a single `sync_all` call (filter `None`). Fold each `SyncOutcome`'s `error` + `warnings` into the existing `warnings` vec sent on `AppEvent::CacheRefresh { warnings }` — no new event field, never abort the poll.
3. Save through the borrow after `sync_all` per RFC-057 §"Save (no flush)" TUI bullet: `store.issue_map`, the per-poll `task_map`, and `status_colors`. Do NOT call `persist_clickup_lifecycles` and do NOT write `.lazyspec.toml`.
4. Remove `refresh_clickup_cache` and its two unit tests (`event_loop.rs`); remove the now-dead per-arm fetch scaffolding.
5. Tests (DICTUM-004, via existing fakes): a poll over a clickup-tasks type writes `status-colors.json`; a per-type fetch failure surfaces on `CacheRefresh { warnings }` without aborting the poll; the poll and `try_push_gh_edit` observe one `issue_map` (no duplicated/drifting copy).

## Out of scope

- The engine seam itself — `sync_all` / `SyncContext` / `Syncers` / the syncers (built in STORY-202 / ITERATION-285); this slice only wires the poll to it.
- Lifecycle persist in the poll (deliberate asymmetry — the poll never rewrites `.lazyspec.toml`; lifecycle persist stays CLI-only per ITERATION-286).
- Any renderer change (foreground ClickUp status-colour rendering already correct per ITERATION-284).
- Poll-latency optimization for the new project-field GraphQL cost (TTL / diff-only injection is a bounded follow-up per RFC-057 Risks, not this slice).

## Principles/conventions

RFC-057 §"Layering (DICTUM-003 / 004)"; CONVENTION; DICTUM-003; DICTUM-004; DICTUM-007.

## Verification

A poll over a clickup-tasks type writes the derived colours to `status-colors.json` (the slice deliverable). The shared `issue_map` has no drifting duplicate — the poll borrows `&mut store.issue_map`, so `try_push_gh_edit` and the `gh_issue_map_stale` reload read the one authoritative map. `refresh_clickup_cache` no longer exists in `event_loop.rs`. `CacheRefresh` still carries only `{ warnings }` (no event-shape change).

