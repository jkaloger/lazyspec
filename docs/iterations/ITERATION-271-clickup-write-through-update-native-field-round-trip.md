---
title: 'ClickUp write-through: update + native field round-trip'
type: iteration
status: complete
author: unknown
date: 2026-07-05
tags: []
related:
- implements: STORY-199
- blocks: ITERATION-272
---

## Objective
`update` (body or attrs) on ClickUp-bound doc -> edit ClickUp task via API; priority/estimate/due round-trip to ClickUp native fields.

## Satisfies
STORY-199 AC2. NFR error classify for edit path.

## Context
- Story+AC: STORY-199. Design: RFC-056 §Design (Field mapping: priority enum, time_estimate ms, due_date epoch ms).
- Builds on ITERATION-270 (write-dir field-map helper, error classify). Read-dir native->attrs helper exists (STORY-198).
- Touch: ClickupTasksStore::update; reuse write-dir helper; refresh TaskMap after edit.

## Tasks
1. Impl ClickupTasksStore::update -> call ClickupClient::task_edit with changed body + mapped attrs.
2. Route priority/estimate/due through write-dir helper to native fields; non-native attrs -> custom field fallback stub only if trivial, else defer.
3. On ok: rewrite cache file + refresh TaskMap.updated_at from response.
4. Fake-client round-trip test: set priority/estimate/due -> task_edit native payload -> read back materializes same values.

## Out of scope
- advance/status push AC3, optimistic-lock AC4.
- Full custom-field attr mapping + relations (RFC story 6).

## Principles/conventions
- CLAUDE.md. RFC-056 §Field mapping (no HTML-comment hack for these 3 — native fields). testing skill.

## Verification
Edit priority+estimate+due -> single task_edit with native payload; subsequent read yields identical attr values (round-trip).
