---
title: Sub-issue read-back on fetch for flat docs
type: iteration
status: in-progress
author: unknown
date: 2026-07-24
tags: []
related:
- implements: STORY-245
---

## Objective

`fetch` reads native sub-issue edges back as configured relation for flat (non-subdir) parents; subdir nesting unchanged.

## Satisfies

STORY-245 AC4 (flat docs → relation injection), AC5 (subdir parents still nest).

## Context

- Story + design: STORY-245 §Read path (split consumption by parent type)
- Existing sub-issue fetch: `fetch_sub_issue_nodes_batch` + resolve src/engine/issue_cache.rs:646, nesting consumer issue_cache.rs:417 (ITERATION-224 behavior)
- Precedent to mirror: dependency read-back (ITERATION-346, `relationship_by_github_native("dependency")` src/engine/issue_cache.rs:429)
- Config lookup: `relationship_by_github_native("sub-issue")`, `subdirectory` flag on type defs
- Prior write path: previous iteration (blocks this one)

## Tasks

1. Test-first: fake GraphQL returning sub-issue edges between two flat issue-docs → fetch injects configured relation (child forward, parent inverse), no subdir nesting (AC4).
2. Split consumption in issue_cache: parent type `subdirectory: true` → nest (today); else if a rel declares `github_native = "sub-issue"` → inject relation; else → today's behavior.
3. Regression test: subdir-type parent still materializes nested docs exactly as ITERATION-224 tests expect (AC5).
4. Removal case: edge gone on remote → relation dropped on re-fetch, no duplicates (mirror dependency read-back drop tests).

## Out of scope

- Write path — previous iteration.
- Nesting-to-relation migration for existing caches — fetch is authoritative, no migration.

## Verification

`show --json` / `status --json` reflect round-tripped relation with correct direction, no output-shape change.
