---
title: clickup_task_type config plumbing and validation
type: iteration
status: complete
author: unknown
date: 2026-07-10
tags: []
related:
- implements: STORY-204
- blocks: ITERATION-290
---

## Objective

Add `clickup_task_type` field to `TypeDef`, round-tripping through `.lazyspec.toml` and `config --json`, valid only on `store = clickup-tasks`.

## Satisfies

STORY-204 AC1, AC2 (numeric `custom_item_id` only — name→id resolution deferred), AC5, AC6, and the config-layer part of AC7 (round-trip + validation-error tests). AC3, AC4 deferred — see Out of scope.

## Context

- Story + ACs: STORY-204
- Design + field semantics (`custom_item_id`, per-type binding): RFC-056 §"Field mapping", §"Config"
- Precedent to mirror: `github_issue_type` — schema-only `TypeDef` field with store guard (grep `github_issue_type` in `src/engine/config.rs`)
- Conventions: docs/convention (dictum 5 ecosystem norms, dictum 6 indirection); `--json` on every command (principle 2)
- Touch: `src/engine/config.rs` (`TypeDef` field + serde + store-mismatch validation, mirror `github_issue_type` guard), config-write CLI mutators / `config add-type` path, README store docs

## Tasks

1. Add `clickup_task_type: Option<i64>` (numeric `custom_item_id`) to `TypeDef`, mirroring `github_issue_type` serde/skip-if-none so it round-trips `.lazyspec.toml` and surfaces in `config --json`.
2. Add validation: `clickup_task_type` set on any `store != clickup-tasks` errors clearly — mirror the existing store/field guard for `github_issue_type` in config.rs.
3. Wire the field through the config-write CLI (`config add-type` and relevant mutators) so it can be set/cleared from the CLI.
4. Test-first: config round-trip (set → `config --json` → reload) and store-mismatch validation error.
5. Update README store docs with the new field, scoped to `clickup-tasks`.

## Out of scope

- Read filter on `custom_item_id` (AC3) → ITER-02.
- Create-payload stamping (AC4) → ITER-02.
- Name→`custom_item_id` API resolution (AC2 stretch) — v1 accepts numeric id only.

## Principles/conventions

- docs/convention dicta (esp. dictum 5 ecosystem norms, dictum 6 no premature indirection).
- Trait-seam + real-by-default I/O (principle 4) — no new trait needed here; pure config.
- Layer separation (principle 3): field lives in engine; CLI only formats.

## Verification

`config --json` shows `clickup_task_type` for a `clickup-tasks` type; setting it on a `filesystem` type errors with a clear store-mismatch message.

