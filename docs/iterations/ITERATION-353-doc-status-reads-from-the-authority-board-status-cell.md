---
title: Doc status reads from the authority board Status cell
type: iteration
status: accepted
author: jkaloger
date: 2026-08-04
tags: []
related:
- implements: STORY-248
- blocks: ITERATION-354
- blocks: ITERATION-355
---

## Objective

A member doc's status comes from its authority-board `Status` cell, not from issue open/closed.

## Satisfies

STORY-248 AC2, AC3, AC9, AC11.

## Context

- Story + AC text: STORY-248. Blocked by ITERATION for AC1/AC8 (lifecycle must exist first).
- Existing status derivation to branch: `src/engine/issue_cache.rs:226`, `:394` (`effective_lifecycle()` → first-active / terminal from open/closed).
- Per-board field values already read as `PROJECT-n.<field>` attrs: STORY-162.
- Warning channel: `outcome.warnings`, printed at `src/cli/fetch.rs:210`.
- Conventions: `DICTUM-004-testing.md`, `DICTUM-006-cli-patterns.md`.
- Touch: `src/engine/issue_cache.rs`, `src/engine/store_dispatch.rs` (github issue sync sites), `src/cli/fetch.rs` (warning plumbing).

## Changes

1. In the issue-cache status derivation, branch on `status_authority`: when set, status = the doc's `Status` cell for the authority board, lowercased via `Status::new`. When unset, current open/closed path unchanged.
2. Source the cell from the `PROJECT-n.Status` value STORY-162 already reads — do not add a second read path.
3. Empty cell → leave status unset; emit an `outcome.warnings` entry naming the doc. Write nothing to the board.
4. Non-authority boards: leave their `Status` as a plain `PROJECT-n.Status` attribute. Only the nominated board feeds status.

## Test Plan

- AC2: member doc, cell `In Progress` → status `in progress`. Assert it is NOT derived from issue open/closed (fake reports a *closed* issue sitting in `In Progress`; status stays `in progress` — this is the drift the story exists to remove).
- AC3: member doc, empty cell → status unset, warning names the doc, zero mutations issued against the board.
- AC9: doc on authority board + a second board that also has `Status` → only the authority board drives status; the other surfaces as `PROJECT-n.Status` attr.
- AC11: warning present in `fetch --json`.

## Notes

- Warning reaches `--json` and stderr now. Routing to the TUI warnings panel is STORY-163 (`draft`) — not a dependency of this slice.
- Non-member docs are out of scope here; whatever they resolve to is settled in the AC4 iteration. Do not add an open/closed fallback — the story forbids two disjoint lifecycles on one type.
- Status stays lowercased end to end. No display-casing work (ADR-023).

## Out of scope

- Adding non-member docs to the board (AC4).
- `update --status` write-through, offline rejection, no-coupling assertion (AC5, AC6, AC7).
- Config key, derive, persist, validate conflict (prior iteration).

