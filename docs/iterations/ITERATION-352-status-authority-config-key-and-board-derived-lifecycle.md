---
title: status_authority config key and board-derived lifecycle
type: iteration
status: complete
author: jkaloger
date: 2026-08-04
tags: []
related:
- implements: STORY-248
- blocks: ITERATION-353
---## Objective

`status_authority` on a type → that board's `Status` options become the type's lifecycle, persisted at fetch.

## Satisfies

STORY-248 AC1, AC8, AC10, AC11.

## Context

- Story + AC text: STORY-248. Parent RFC: RFC-050.
- Precedent to mirror: `derive_lifecycle` (`src/engine/clickup_cache.rs:168`), `persist_clickup_lifecycles` (`src/cli/fetch.rs:276`).
- Conventions: `docs/convention/DICTUM-004-testing.md` (TDD, fakes at trait seams), `DICTUM-006-cli-patterns.md` (`--json`), `DICTUM-002-trait-usage.md`.
- Touch: `src/engine/config.rs` (TypeDef field), `src/engine/gh_schema.rs` (wire existing `fetch_project_fields`), `src/engine/issue_cache.rs:287` (snapshot fetch site), `src/cli/fetch.rs` (persist), `src/engine/validation.rs` (conflict error), `README.md` + config reference.

## Changes

1. `TypeDef.status_authority: Option<String>` in `src/engine/config.rs`. `#[serde(default)]`, names a `github-projects` doc id (`PROJECT-7`). `JsonSchema` derive already on the struct → RFC-058 schema covered, no separate schema edit.
2. Wire `fetch_project_fields(gh, repo, project_number)` (`gh_schema.rs:165`) into the snapshot for each type's authority board, merging `project_fields` / `single_select_options` / `iterations`. **No caller today** — only `fetch_snapshot` is called (`issue_cache.rs:287`) and it fetches issue types only, so these snapshot arrays are always empty. Resolve `project_number` from the nominated project doc (STORY-161 cached board binding).
3. Derive fn: `single_select_options` for `(project_number, "Status")` → `Lifecycle { states: option names lowercased, edges: [] }`. Mirror `clickup_cache.rs:168`.
4. Persist at fetch — sibling of `persist_clickup_lifecycles`, same in-place rewrite, same write-only-on-change.
5. `validation.rs`: error when `status_authority` set **and** `lifecycle.states` non-empty. Name both keys + the type.
6. README + config reference: the key, and that only the nominated board is authoritative (other boards stay `PROJECT-n.Status` attrs).

## Test Plan

- AC1: fake `GhGraphql` returns Status options `Ready To Start` / `In Progress` / `Review` / `Done` → states `["ready to start","in progress","review","done"]`, that order, `edges` empty. `config --json` reports them.
- AC1 idempotence: second fetch, unchanged board → `.lazyspec.toml` untouched (mirrors `persist_clickup_lifecycles_leaves_config_untouched_when_unchanged`).
- AC8: type with both keys → `validate` errors naming both.
- AC10 regression: `github-issues` / `github-milestones` type with no `status_authority` → `open`/`closed` unchanged. Guard STORY-224 AC1, AC2, AC3, AC6.
- AC11: `--json` on `config`, `fetch`, `validate`.

## Notes

- **Vec order IS board order.** `OptionId` carries no index field (`gh_schema.rs:23`); GraphQL returns options in board order. Do not sort.
- **Lowercase is forced, not stylistic** — see STORY-248 Context. Enforced at `src/engine/document.rs:98`, `:119` and `src/engine/config.rs:1315`. Persisting `In Progress` → every board status fails membership.
- `effective_lifecycle()` **unchanged**. Persisted lifecycle returns via its existing declared-states branch. Do not touch the 28 call sites.
- No cell reading in this slice — lifecycle only. Doc statuses still come from open/closed until ITERATION 2.

## Out of scope

- Reading a doc's status from its Status cell, empty-cell warning → next iteration (AC2, AC3, AC9).
- Adding non-member docs to the board (AC4).
- `update --status` write-through (AC5, AC6, AC7).
