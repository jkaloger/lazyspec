---
title: 'ClickUp write-through: create'
type: iteration
status: draft
author: unknown
date: 2026-07-05
tags: []
related:
- implements: STORY-199
- blocks: ITERATION-271
---

## Objective
`create` on ClickUp-bound doc -> new ClickUp task via API, mirror to local cache.

## Satisfies
STORY-199 AC1. NFR (reqwest/HTTP error classify) for create path.

## Context
- Story+AC: STORY-199. Design: RFC-056 §Design (ClickupTasksStore, Field mapping, Caching/id-mapping).
- Assume done (STORY-198): registry dispatch, ClickupClient trait+reqwest+fake, read path, TaskMap type, cache helpers.
- Touch: ClickupTasksStore::create; write-dir field-map helper (attrs->native payload); TaskMap write; reqwest error classify helper.
- Field mapping is asymmetric read vs write (RFC-056 §Field mapping) — the write-dir helper emits write-shape values, distinct from the read decoder:
  - `priority` -> bare integer `1=Urgent 2=High 3=Normal 4=Low` (`null` clears); NOT the read-side object.
  - `due`/`start`/`estimate` -> integer epoch-ms / ms; read side returns strings.
  - body -> `markdown_content` (takes precedence over `description`).
  - `custom_item_id`: when the bound List uses custom task types, send its `custom_item_id`; custom-field values are only persisted when applicable to it.

## Tasks
1. Impl ClickupTasksStore::create -> build payload (name, `markdown_content` body, status; priority/estimate/due -> native write-shape via new write-dir helper; `custom_item_id` when the List uses custom task types), call ClickupClient::task_create.
2. On ok: insert TaskMap entry (task id + updated_at from response `date_updated`), write cache file via reused write_cache_file.
3. Classify task_create errors by reqwest::Error variant + HTTP status. No stderr-substring scrape (see RFC wart re gh.rs classify_gh_error).
4. Fake-client test: create -> task_create called with mapped payload, cache + TaskMap written.

## Out of scope
- update AC2 (edit path, native round-TRIP read-back), advance AC3, optimistic-lock AC4.
- Relations custom field (RFC story 6).

## Principles/conventions
- CLAUDE.md (cargo run dev, --json). RFC-056 §Transport error-classify posture. testing skill (fake client, TDD).

## Verification
Create doc bound to List -> exactly one task_create call, TaskMap.updated_at set from response, cache file materialized.
