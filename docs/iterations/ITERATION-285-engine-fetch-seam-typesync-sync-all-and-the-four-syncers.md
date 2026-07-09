---
title: 'Engine fetch seam: TypeSync, sync_all, and the four syncers'
type: iteration
status: complete
author: jkaloger
date: 2026-07-09
tags: []
related:
- implements: STORY-202
- blocks: ITERATION-286
- blocks: ITERATION-287
---

## Objective

Introduce the engine fetch seam — `TypeSync` contract, borrowed `SyncContext`, `SyncOutcome`, `Syncers`, and a never-aborting `sync_all` that dispatches over `match StoreBackend` — plus all four syncers, driven entirely at the engine level by the existing client fakes.

## Context

- Story + ACs: STORY-202 (the engine-side ACs; see Satisfies).
- Design (module layout, `TypeSync` contract, `SyncContext`/`GhMaps`/`ClickupMaps`, `SyncOutcome`, `Syncers`, `sync_all` dispatch + ordering + error model): RFC-057 §"Design" (`TypeSync` contract + borrowed `SyncContext`, Dispatch, Errors, Save, Layering) and §"Interfaces".
- Conventions/dictums that govern the work: CONVENTION, DICTUM-002 (Trait Usage — why `TypeSync` stays non-`dyn`), DICTUM-003 (Module Structure — engine holds no I/O assumptions; clients injected), DICTUM-004 (Testing — drive syncers through existing client seams with fakes).
- Wrapped unchanged (do not rewrite): `engine::milestone_cache::fetch_milestones`, `engine::issue_cache::IssueCache::fetch_all`, `engine::clickup_cache::fetch_tasks` / `fetch_lifecycle_and_colors`, `engine::store_dispatch::inject_project_fields_for_meta` / `write_cache_file`.
- Relocate: `fetch_git_ref_type` (currently `src/cli/fetch.rs:360`, returns CLI-private `TypeSummary`).
- Touch: `src/engine/sync.rs` (new), `src/engine.rs`/`src/engine/mod` (register module), `src/engine/config.rs` (`StoreBackend`, six variants — read only), `src/engine/status_colors.rs` / `issue_map.rs` / `task_map.rs` / `git_ref.rs` (sidecar map + ops types the context borrows and the syncers hold).

## Satisfies

STORY-202: ordering constraint (milestones-before-issues, established inside `sync_all`) and the cross-relation resolution AC; project-field injection folded into `GhIssueSync`; the injection-failure → `warning`-not-`error` classification AC (engine records the warning; the exit-zero half is ITERATION-286); ClickUp colour capture to `status-colors.json` and lifecycle returned in the outcome (the `.lazyspec.toml` persist half is ITERATION-286); filesystem / github-projects explicit skip-arm AC; configured-backend-without-a-syncer → `error`-bearing `SyncOutcome` (not panic) AC.

## Tasks

1. Create `src/engine/sync.rs` and register it; define `SyncContext<'a>`, `GhMaps<'a>`, `ClickupMaps<'a>`, `SyncOutcome`, and the `TypeSync` static (non-`dyn`) trait exactly per RFC-057 §"`TypeSync` contract + borrowed `SyncContext`". No `Result` on `sync`; failure lives in `SyncOutcome.error`.
2. Implement `GhMilestoneSync` and `GhIssueSync` wrapping the retained cache fns; fold the project-field injection (body of CLI-only `inject_project_fields_into_cache`) into `GhIssueSync` after `fetch_all`, best-effort per RFC-057 (GraphQL failure → `warnings`, cached doc keeps other fields). Both mutate `ctx.gh`.
3. Relocate `fetch_git_ref_type` into `GitRefSync` and have it return `SyncOutcome`; implement `ClickupSync` (`fetch_tasks` + `fetch_lifecycle_and_colors` + `status_colors.set_type`, mutating `ctx.clickup`, returning derived lifecycle in the outcome).
4. Define `Syncers` (per-backend `Option` fields) and `sync_all` per RFC-057 §"Dispatch"/§"Errors": fixed backend order (milestones, issues, git-ref, clickup), `match StoreBackend` with explicit Filesystem/GithubProjects skip arms, single-type `filter`, never abort, missing-syncer-for-configured-backend → `error`-bearing outcome.
5. Tests at the engine seam via existing fakes (DICTUM-004): ordering (milestone + issue with a cross relation resolves after `sync_all`); injection failure → warning not error; missing syncer → error not panic; skip arms produce no outcome/no-op.

## Out of scope

- All CLI wiring: `fetch::run` migration, `TypeSummary` removal, `--json` `error` field, exit codes, run-local map save, `persist_clickup_lifecycles`, and the "no fetchable types" message → ITERATION-286 (STORY-202 CLI-surface ACs, incl. the exit-zero-on-warning and `.lazyspec.toml`-persist halves).
- TUI poll migration → STORY-203.
- Milestone→issue dependency guard (RFC-057 Non-goals): `sync_all` does not skip issue fetch on milestone failure.

## Principles/conventions

RFC-057 §"Layering (DICTUM-003 / 004)"; CONVENTION principle 6 (four impls justify the contract); DICTUM-002, DICTUM-003, DICTUM-004.

## Verification

`match StoreBackend` in `sync_all` is exhaustive over all six variants with Filesystem/GithubProjects as skip arms (compiler-enforced). The ordering test fails if issue fetch is dispatched before milestone fetch.

