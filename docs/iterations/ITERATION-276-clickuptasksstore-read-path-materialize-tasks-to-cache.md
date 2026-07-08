---
title: 'ClickupTasksStore read path: materialize tasks to cache'
type: iteration
status: draft
author: unknown
date: 2026-07-05
tags: []
related:
- implements: STORY-198
- blocks: ITERATION-277
---

Objective: fetch a bound List tasks and write each to .lazyspec/cache/<type>/<ID>.md in the github-issues cache shape.

Refs: RFC-056 Design sections ClickupTasksStore, Field mapping, Caching/id-mapping; IssueMap shape; write_cache_file / write_cache_parent / write_cache_child + CacheLock (store_dispatch.rs); github-issues cache doc shape.

Satisfies: STORY-198 AC1, AC2, AC4 + the TaskMap non-functional. AC3 deferred.

Tasks:
1. ClickupTasksStore: non-generic struct holding a boxed ClickupClient, registered under StoreBackend::ClickupTasks in the registry from the refactor iteration.
2. TaskMap at .lazyspec/task-map.json mirroring IssueMap (task id, updated_at, node id); reuse CacheLock.
3. Read side: fetch tasks for the bound clickup_list_id (GET /list/{id}/task, paginated 100/page), map to DocMeta where status is the raw ClickUp status verbatim and priority/estimate/due come from native fields. Read shapes (RFC-056 §Field mapping; asymmetric vs write): priority is an object `{"priority":"normal",..}` -> decode the `priority` string; due_date/start_date/time_estimate/date_updated arrive as epoch-ms/ms *strings* -> deser string-or-int; body from markdown_description/text_content; custom_fields keyed by uuid.
4. Write cache via write_cache_file / write_cache_parent / write_cache_child unchanged.
5. lazyspec fetch works end-to-end read-only.

Out of scope: AC3 lifecycle-from-list (next iteration); write path create/update/advance (RFC story 5); relations (RFC story 6); no local status mapping table.

Principles: reuse cache helpers unchanged; freshness/staleness parity with github-issues.

AC:
- Given a type with clickup_list_id and a valid token, when lazyspec fetch runs, then each List task materializes to a cache file in the github-issues shape.
- Given a fetched task, then status is the raw ClickUp status string and priority/estimate/due come from ClickUp native task fields.
- Given fetched docs, when status --json and show <ID> --json run, then they behave identically to github-issues docs including freshness/staleness.
