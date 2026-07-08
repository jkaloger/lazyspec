---
title: Write through create, update and advance to ClickUp
type: story
status: in-progress
author: unknown
date: 2026-07-05
tags: []
related:
- implements: RFC-056
---

As a developer, I want create/update/advance on a ClickUp-backed doc to write through to the ClickUp task, so lazyspec is a full read-write client in the same class as `ticket` (not read-only).

Implements RFC-056 (ClickUp store). Journey step: act + manage. Depends on the read-skeleton story (registry + cache + TaskMap must exist first).

## Acceptance criteria

- Given a type bound to a ClickUp List, when I run `lazyspec create`, then a new ClickUp task is created via the ClickUp API and mirrored to the local cache.
- Given an existing ClickUp-backed doc, when I run `update` (body or attributes), then the ClickUp task is edited via the API, and `priority`/`estimate`/`due` round-trip to ClickUp's native fields.
- Given `advance`, when I move the doc's status, then the raw ClickUp status string is pushed to ClickUp; lazyspec does not duplicate ClickUp's transition gating (ClickUp enforces its own rules).
- Given a locally stale doc, when a write races an external ClickUp change, then the write is rejected via optimistic-lock on `TaskMap.updated_at`.
- Given a ClickUp-backed doc, when I delete it, then the ClickUp task is archived (`archived: true`), never hard-deleted, and the doc drops from the local cache on next sync.

## Enabler acceptance criteria (folded from RFC-056 story 0)

- Given the write path, then it dispatches through the non-generic store registry introduced by the read-skeleton story; no new generic type param is added to `dispatch_for_type`.

## Non-functional

- Errors classified by real `reqwest::Error` variants and HTTP status codes, not stderr-substring scraping.
