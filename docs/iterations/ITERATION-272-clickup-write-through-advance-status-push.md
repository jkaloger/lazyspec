---
title: 'ClickUp write-through: advance status push'
type: iteration
status: complete
author: unknown
date: 2026-07-05
tags: []
related:
- implements: STORY-199
- blocks: ITERATION-273
---

## Objective
`advance` -> push raw ClickUp status string to the task; no local transition gating (ClickUp enforces its own rules).

## Satisfies
STORY-199 AC3.

## Context
- Story+AC: STORY-199. Design: RFC-056 §Status handling (raw status string verbatim, lifecycle derived from bound List, no local edges/gating).
- Builds on ITERATION-271 (task_edit path + TaskMap refresh). Type lifecycle states already populated from List status set (STORY-198 read path).
- Touch: ClickupTasksStore::update status field OR advance wiring; ensure no lazyspec-side edge check duplicates ClickUp.

## Tasks
1. On advance, send target status as raw ClickUp status string via task_edit(status).
2. Do NOT run local lifecycle edge/gate validation for ClickUp-backed types — defer entirely to ClickUp (same posture as ticket empty edges).
3. On ok: rewrite cache status + refresh TaskMap.updated_at.
4. Fake-client test: advance to a valid List status -> task_edit(status=raw) called; no local gating rejection.

## Out of scope
- create AC1, update body/attrs AC2, optimistic-lock AC4.

## Principles/conventions
- CLAUDE.md. RFC-056 §Status handling. Error classify posture from ITERATION-270.

## Verification
Advance ClickUp doc -> raw status string pushed verbatim; lazyspec applies no transition gate of its own.
