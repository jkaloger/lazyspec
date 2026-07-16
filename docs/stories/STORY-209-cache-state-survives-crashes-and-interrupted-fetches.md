---
title: Cache state survives crashes and interrupted fetches
type: story
status: accepted
author: agent
date: 2026-07-16
tags: []
related:
- related-to: AUDIT-018
---

## Value

As a user with github-issues or clickup-tasks backends, my local cache state survives crashes and interrupted fetches — no silent loss of freshness metadata or cached docs.

## Acceptance Criteria

- AC1: Given a corrupt (present but unparseable) `cache.lock`, when any cache operation runs, then it fails with a clear error and never persists a defaulted-empty lock; an absent file still yields a clean default. (AUDIT-018 C1)
- AC2: All sidecar persistence (`cache_lock`, `issue_map`, `task_map`, sync state, dispatch state) writes via one shared temp-file+rename atomic helper. (AUDIT-018 C5 mechanics)
- AC3: Given `fetch_all` fails partway (e.g. disk full), then the previously cached docs for that type remain intact — staging dir + swap or write-then-delete-stale. Applies to both `issue_cache` and `clickup_cache`. (AUDIT-018 C4)

## Out of scope

Advisory interprocess file locking (C5 lock half) — separate story if multi-process contention shows up in practice.

