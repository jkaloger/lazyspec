---
title: Fetch adds non-member docs to the authority board
type: iteration
status: complete
author: jkaloger
date: 2026-08-04
tags: []
related:
- implements: STORY-248
---

## Objective

Fetch adds a doc that is not an item of the authority board onto it, so the type never reports two lifecycles.

## Satisfies

STORY-248 AC4.

## Context

- Story + AC text: STORY-248. Blocked by the AC2/AC3 iteration (the added doc lands in AC3's unset+warning state).
- Mutation already built: `addProjectV2ItemById(projectId, contentId)` — STORY-161.
- Conventions: `DICTUM-004-testing.md`, `DICTUM-002-trait-usage.md`.
- Touch: `src/engine/issue_cache.rs` / `src/engine/store_dispatch.rs` (fetch path for `github-issues` types), fake at the `GhGraphql` seam.

## Changes

1. On fetch of a type with `status_authority`: for each doc whose issue is not an item of the authority board, call `addProjectV2ItemById` with the board's project node id (cached by STORY-161) and the issue content id.
2. Added item has an empty `Status` cell → resolves through the AC3 path (unset + warning). No seeding of a first column.
3. No open/closed fallback for these docs, before or after the add.

## Test Plan

- AC4: type with 2 docs, one already an item, one not → exactly one `addProjectV2ItemById` call, for the non-member. Post-add the doc's status is unset with a warning.
- AC4: after the add, neither doc ever reports `open` or `closed`.
- Idempotence: second fetch, both now members → zero `addProjectV2ItemById` calls.

## Notes

- **This makes fetch mutate the board** — the only write on a read path in this story. Reviewed as its own slice for that reason.
- Membership check must come from the item list already read for `PROJECT-n.<field>` values (STORY-162), not a fresh probe per doc.
- Last-write-wins + refresh, per RFC-050. No conflict detection.

## Out of scope

- Seeding the `Status` cell of a newly added item (story chose unset + warning).
- Removing docs from the board, or reordering board items.
- `update --status` write-through (AC5, AC6, AC7).

