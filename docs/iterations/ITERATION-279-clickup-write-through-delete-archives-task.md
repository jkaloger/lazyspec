---
title: 'ClickUp write-through: delete archives task'
type: iteration
status: draft
author: unknown
date: 2026-07-08
tags: []
related:
- implements: STORY-199
---

## Objective
`delete` on ClickUp-bound doc -> archive ClickUp task (`archived: true`), never hard-delete; doc leaves cache on next sync.

## Satisfies
STORY-199 AC5.

## Context
- Story+AC: STORY-199. Design: RFC-056 §Design (ClickupTasksStore delete semantics, Transport endpoint table: task_archive -> PUT /task/{id} {"archived":true}; DELETE /task/{id} stays unused).
- Assume done: ClickupClient trait incl. task_archive (ITERATION-268), read path + TaskMap + cache sync (ITERATION-276), write-through create/update (ITERATION-270/271).
- Touch: ClickupTasksStore::delete; ClickupClient::task_archive real+fake impls if stubbed.

## Tasks
1. Impl ClickupTasksStore::delete -> ClickupClient::task_archive (PUT /task/{id} archived:true). No DELETE endpoint call anywhere.
2. Do NOT eagerly delete cache file/TaskMap entry — archived tasks drop from task_list fetch, so next sync removes doc from cache (RFC-056 §ClickupTasksStore). Verify sync path handles disappearance.
3. Classify errors by reqwest::Error variant + HTTP status, same helper as create/update.
4. Fake-client test: delete -> exactly one task_archive call with archived:true; no hard-delete method exists/called; subsequent sync without task removes cache file + TaskMap entry.

## Out of scope
- AC1-AC4 (covered by ITERATION-270..273). Relations custom field (STORY-200).
- Any `DELETE /task/{id}` support.

## Principles/conventions
- CLAUDE.md (cargo run dev, --json). RFC-056 §Transport error-classify posture. testing skill (fake client, TDD).

## Verification
Delete ClickUp-bound doc -> one PUT /task/{id} with archived:true, zero DELETE calls; next sync drops doc from cache.

