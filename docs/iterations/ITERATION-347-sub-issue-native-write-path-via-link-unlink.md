---
title: Sub-issue native write path via link/unlink
type: iteration
status: in-progress
author: unknown
date: 2026-07-24
tags: []
related:
- implements: STORY-245
- blocks: ITERATION-348
---

## Objective

`link`/`unlink` write GitHub-native sub-issue edge for relationship with `github_native = "sub-issue"`.

## Satisfies

STORY-245 AC1 (link → addSubIssue), AC2 (unlink → removeSubIssue), AC3 (single-parent error), AC6 (opportunistic no-op), AC7 (config validation ≤1 sub-issue rel), AC8 (fake seam tests). AC4, AC5 deferred — see Out of scope.

## Context

- Story + design: STORY-245 (direction: source = child, target = parent)
- Precedent to mirror: `apply_native_dependency` src/engine/ops/link.rs:355 (dispatch shape, opportunistic guard, resync return)
- Mutations exist: `ADD_SUB_ISSUE_MUTATION` / `REMOVE_SUB_ISSUE_MUTATION` src/engine/gh_subissue.rs
- Node ids: `IssueMap` src/engine/issue_map.rs (`node_id`, may be empty on legacy maps — treat empty as error telling user to re-fetch)
- Config: `github_native: Option<String>` src/engine/config.rs:398, `relationship_by_github_native` config.rs:1246
- Wire points: `link_inner` src/engine/ops/link.rs:99, unlink path link.rs:448, `resync_after_native_edge` link.rs:684
- Convention: docs/convention (traits at I/O seams, fakes at seams only)

## Tasks

1. Config validation: reject config where >1 relationship declares `github_native = "sub-issue"` (STORY-245 AC7). Test first.
2. `apply_native_subissue` in src/engine/ops/link.rs mirroring `apply_native_dependency`: guard (rel is sub-issue native + both endpoints same-repo github-issues), resolve node ids from IssueMap, link → `addSubIssue(issueId: target-node, subIssueId: source-node)`, unlink → `removeSubIssue`, return `true` → resync. Fake at `GhGraphql` seam, test-first.
3. Single-parent guard: before addSubIssue, query child's existing parent (GraphQL `parent` field on Issue); if set and ≠ target, fail naming existing parent, no mutation (AC3).
4. Wire into `link_inner` + unlink path. Opportunistic fall-through tests: filesystem/cross-store endpoints → no native call, relation recorded (AC6).

## Out of scope

- Fetch read-back (STORY-245 AC4, AC5) → next iteration.
- Subdir-structural path changes — none.
- Reparent semantics — explicit non-goal in STORY-245.

## Verification

Empty `node_id` in issue map → clear error, no mutation. Config with `implements` sub-issue-native + filesystem-only docs → all existing tests green (regression-free).
