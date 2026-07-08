---
title: 'ClickUp write-through: optimistic-lock on TaskMap.updated_at'
type: iteration
status: draft
author: unknown
date: 2026-07-05
tags: []
related:
- implements: STORY-199
- blocks: ITERATION-279
---

## Objective
Reject a write when local doc is stale vs ClickUp — optimistic-lock on TaskMap.updated_at across update/advance.

## Satisfies
STORY-199 AC4.

## Context
- Story+AC: STORY-199. Design: RFC-056 §Caching/id-mapping (TaskMap.updated_at for optimistic-lock).
- Builds on ITERATION-271 + ITERATION-272 (edit/status write paths exist).
- Touch: pre-write guard in ClickupTasksStore edit/status paths; TaskMap.updated_at compare.
- `TaskMap.updated_at` maps to ClickUp's `date_updated`, returned as an epoch-ms *string* (`"1774587145901"`) — deser must accept string-or-int; compare as integers, not string equality.

## Tasks
1. Before edit/status write, fetch current ClickUp task `date_updated`; compare (as integer epoch-ms) to stored TaskMap.updated_at.
2. If remote newer than local -> reject write with a stale/conflict error (not a silent overwrite).
3. On accepted write -> refresh TaskMap.updated_at from response `date_updated`.
4. Fake-client test: simulate external change (remote updated_at advanced) -> write rejected; no external change -> write proceeds + updated_at refreshed.

## Out of scope
- create AC1 (new task, no prior updated_at to race).
- Relations (RFC story 6).

## Principles/conventions
- CLAUDE.md. RFC-056 §Caching/id-mapping. Error classify posture from ITERATION-270 (conflict is a classified error, not stderr scrape).

## Verification
Stale doc racing external ClickUp change -> write rejected on updated_at mismatch; fresh doc -> write succeeds.
