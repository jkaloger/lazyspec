---
title: update --status writes through to the board Status cell
type: iteration
status: accepted
author: jkaloger
date: 2026-08-04
tags: []
related:
- implements: STORY-248
---

## Objective

`update --status` moves the card: writes the authority board's `Status` cell, leaves issue open/closed alone.

## Satisfies

STORY-248 AC5, AC6, AC7, AC11.

## Context

- Story + AC text: STORY-248. Blocked by the AC2/AC3 iteration (status must read from the cell before it can round-trip).
- Write path already built: `updateProjectV2ItemFieldValue`, single-key `value` object — STORY-162.
- Option ids: `OptionId { field_id, name, id }` (`src/engine/gh_schema.rs:23`), loaded offline by `GhSchemaSnapshot::load` (`:57`).
- Transition gate reads `effective_lifecycle()` at `src/engine/ops/update.rs:24`.
- Conventions: `DICTUM-004-testing.md`, `DICTUM-006-cli-patterns.md`, `DICTUM-007-tui-patterns.md` (status picker).
- Touch: `src/engine/ops/update.rs`, `src/engine/store_dispatch.rs` (github write-through), `src/engine/gh_schema.rs` (option lookup helper).

## Changes

1. Snapshot lookup: requested status → `singleSelectOptionId`, matching the board's option `name` **case-insensitively** (states are lowercased, board names are not).
2. When the type has `status_authority`, route the status write to `updateProjectV2ItemFieldValue` with a `value` object carrying exactly `singleSelectOptionId`.
3. Unmatched status → reject before any mutation, naming the valid options.
4. Suppress the open/closed write-through for `status_authority` types — in both directions.

## Test Plan

- AC5: doc at `ready to start`, `update --status "In Progress"` → one `updateProjectV2ItemFieldValue` with exactly one key (`singleSelectOptionId`), correct option id; cached status becomes `in progress`. Same for `"in progress"` (both lowercase to one state).
- AC6: `--status "Blocked"` (no such option) → rejected offline, valid options named, **zero** GraphQL calls. Run with no network reachable.
- AC7: move to the last column in board order → issue open/closed untouched, no `updateIssue`/close call. Move back out → no reopen call.
- AC11: `update --json`.

## Notes

- **No open/closed coupling by design.** Teams express "Done closes the issue" as Projects automation on the board; duplicating it here would fight that automation. This is why AC7 is asserted as the *absence* of a call.
- Transition gate needs no change: `effective_lifecycle()` already returns the persisted board states, and the empty edge set leaves every column reachable from every other.
- TUI status picker gets board columns for free via `effective_lifecycle()` (`src/tui/state/app.rs:2920`), offline from the snapshot.
- Last-write-wins + refresh, per RFC-050.

## Out of scope

- Ordered transition rules between columns — empty edges are deliberate; a board carries order but no transition rules.
- Clearing the `Status` cell (`clearProjectV2ItemFieldValue`); no lazyspec status means "unset", which this slice never writes.
- Preserving board display casing in a doc's status or the TUI (ADR-023).

