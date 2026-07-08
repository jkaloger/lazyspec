---
title: TUI background poll refreshes clickup-tasks store (github parity)
type: iteration
status: complete
author: unknown
date: 2026-07-09
tags: []
related:
- implements: STORY-198
---## Objective

TUI background poll refreshes `clickup-tasks` cache live, same as github-issues/milestones — no manual `lazyspec fetch` before docs appear.

## Context

- Story: STORY-198 (read path). RFC-056 §Status handling.
- Root cause: poll gated `has_pollable_gh_types` (github only); poll thread filters `GithubIssues`/`GithubMilestones` only. No ClickUp arm. `Store::load` cold-cache fallback exists for `GitRef` only, not `ClickupTasks`.
- Touch: src/tui/infra/event_loop.rs (gate :126,:374,:399; poll-run block :504; poll thread :513-586).
- Ref pattern: CLI fetch clickup arm src/cli/fetch.rs:188-230 (token load, `clickup_cache::fetch_tasks`, TaskMap). Token+client build main.rs:102-133 (`LayeredCredentialStore::global().load_clickup_token()`, `ClickupHttpClient`).

## Satisfies

STORY-198 read path — extends to TUI live-refresh (parity with github poll). No new AC; closes the CLI-vs-TUI gap.

## Tasks

1. Poll trigger must fire for clickup-only projects. Today the poll-run block (:504) gates on BOTH `next_poll` AND `shared_gh_store.is_some()`; a clickup-only project has no github repo → `shared_gh_store` None → poll never runs. Restructure: gate poll scheduling (:399 `next_poll`) and the run block (:504) on 'any pollable type' (github OR clickup), not on `shared_gh_store` presence. Generalize `has_pollable_gh_types` → `has_pollable_types` (add `ClickupTasks`). `shared_gh_store` stays optional and its github arm runs only when Some.
2. Add clickup arm to poll thread (:513-586), independent of `shared_gh_store`: `load_clickup_token()`; if absent → one warning, skip (github still polls). Build `ClickupHttpClient`; `TaskMap::load`; loop `ClickupTasks` types → `clickup_cache::fetch_tasks`; `TaskMap::save`. Per-type Err → push warning (mirror gh arm), never crash.
3. Warnings ride existing `CacheRefresh` event (already reloads `Store`). No new event/struct.
4. Tests: `has_pollable_types` true for clickup-only config (mirror :750/:762). Poll clickup arm token-absent → warning, no panic. Fake `ClickupClient` at the seam.

## Out of scope

- Config lifecycle rewrite from TUI (fetch.rs `persist_clickup_lifecycles` / `fetch_lifecycle`) — rewriting .lazyspec.toml mid-session churns the config watcher. Cache refresh only. Defer.
- Write-through (STORY-199). TaskMap concurrency with write-through path (human-initiated, infrequent) — poll load/save is last-writer-wins, acceptable for this slice.

## Principles

- Layering: network/token I/O stays in TUI arm, not engine `Store::load` (dictum 3).
- Traits at seam: `ClickupClient` trait, fake in tests (dictum 4).

## Verification

Configure clickup-tasks type + token on a project with NO github repo, launch TUI cold (no cache) → tasks appear within one poll TTL, no manual fetch. Confirms trigger decoupled from `shared_gh_store`.