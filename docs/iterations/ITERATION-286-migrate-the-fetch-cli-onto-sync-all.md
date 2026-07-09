---
title: Migrate the fetch CLI onto sync_all
type: iteration
status: complete
author: jkaloger
date: 2026-07-09
tags: []
related:
- implements: STORY-202
---

## Objective

Migrate `lazyspec fetch` off its bespoke store-filtered loops onto the engine's `sync_all`, replacing the CLI-private `TypeSummary` with `SyncOutcome` and adopting the continue-then-exit-non-zero error model.

## Context

- Story + ACs: STORY-202 (the CLI-surface ACs; see Satisfies).
- Design (caller wiring, save asymmetry, error model, `--json` `error` field): RFC-057 §"Caller wiring" (CLI bullet), §"Save (no `flush`)" (CLI bullet), §"Errors" (CLI bullet), §"Interfaces" (CLI surface).
- Depends on the engine seam from ITERATION-285 (`sync_all`, `Syncers`, `SyncContext`, `SyncOutcome`).
- Conventions/dictums: DICTUM-006 (CLI Patterns — `--json`, exit codes, output shape), DICTUM-003 (CLI depends only on engine), CONVENTION principle 2 (`--json` everywhere).
- Touch: `src/cli/fetch.rs` (`run` at :20; remove `TypeSummary` at :435, `inject_project_fields_into_cache` at :265 — body already moved in ITERATION-285, `fetch_git_ref_type` at :360 — relocated in ITERATION-285; keep/relocate `persist_clickup_lifecycles` at :321 per RFC-057 Interfaces; keep the "no fetchable types" branch at :69 and `filter_types` at :352).

## Satisfies

STORY-202: every type refreshes through `sync_all` when `lazyspec fetch` runs (end-to-end); all-succeed → everything persisted, exit zero; one-failure → remaining types still refresh, error prints, successes persisted, exit non-zero; token-absent / repo-unresolvable → hard-error while *building* `Syncers` (before any sync), distinct from a per-type `SyncOutcome.error`; injection-failure → exits zero (a warning is not a failure); `--json` gains optional `"error"` on failed entries while successful entries keep the exact `{type,fetched,new,removed}` shape, exit non-zero; ClickUp derived lifecycles persisted to `.lazyspec.toml`; `--type T` filters to one type; the "no fetchable types" message.

## Tasks

1. In `fetch::run`, load run-local `issue_map` / `task_map` / `status_colors`; build a `SyncContext` borrowing them (populate `gh`/`clickup` only for configured backends); build `Syncers` with real clients/tokens — token-absent / repo-unresolvable raise here as `?`-hard-errors, before `sync_all`. Preserve the "no fetchable types" branch and `--type` via `sync_all`'s `filter`.
2. Call `sync_all`; drop `TypeSummary` and thread `SyncOutcome` through the summary/`--json` printing. Per RFC-057 §"Interfaces": failed `--json` entries add `"error": "<msg>"`, successful entries keep `{type,fetched,new,removed}` unchanged.
3. After `sync_all`: save run-local `issue_map` + `task_map` + `status_colors`, then `persist_clickup_lifecycles` from the outcomes' `lifecycle` values. Exit non-zero iff any outcome has `error`; a `warnings`-only run exits zero.
4. Update tests in `fetch.rs` for the new flow; update the README `lazyspec fetch` section for the `--json` `error` field and the continue-then-exit-non-zero behaviour.

## Out of scope

- Engine seam internals (owned by ITERATION-285).
- TUI poll migration → STORY-203.
- Renderer/status-colour output changes (stays as ITERATION-284 shipped).
- Milestone→issue dependency guard (RFC-057 Non-goals) — the CLI now proceeds into issue fetch past a milestone failure against a stale map; accepted, not a regression.

## Principles/conventions

DICTUM-006 (CLI patterns), DICTUM-003 (layering), CONVENTION principle 2.

## Verification

Successful `--json` entries are byte-for-byte the pre-migration `{type,fetched,new,removed}` shape (no spurious `error` key). A run with one failing type exits non-zero yet still persists every succeeding cache. A run with only an injection warning exits zero. Repo-unresolvable aborts before any cache is written.

