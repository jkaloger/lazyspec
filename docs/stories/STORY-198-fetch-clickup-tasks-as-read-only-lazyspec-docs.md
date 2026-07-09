---
title: Fetch ClickUp tasks as read-only lazyspec docs
type: story
status: complete
author: unknown
date: 2026-07-05
tags: []
related:
- implements: RFC-056
---

As a developer, I bind a lazyspec type to a ClickUp List and fetch its tasks as read-only structured docs, so ClickUp-tracked work enters lazyspec's --json pipeline.

Implements RFC-056 (ClickUp store). Journey step: discover. This is the walking skeleton: thinnest end-to-end read-only slice.

## Acceptance criteria

- Given a type configured with `clickup_list_id` and a valid token, when I run `lazyspec fetch`, then each task in the bound List materializes to `.lazyspec/cache/<type>/<ID>.md` with the same cache file shape as github-issues docs.
- Given a fetched task, then the doc `status` is the raw ClickUp status string verbatim (no local mapping table), and `priority`/`estimate`/`due` are read from ClickUp's native task fields (priority enum, `time_estimate` in ms, `due_date` epoch ms).
- Given the bound List, then the type's effective lifecycle states are populated from the List's status set at sync time, with no local edges or gating (same posture `ticket` takes with empty lifecycle).
- Given fetched docs, when I run `status --json` and `show <ID> --json`, then they behave identically to github-issues docs, including freshness/staleness handling.

## Enabler acceptance criteria (RFC-056 story 0 — dispatch registry refactor, folded here)

- Given the new backend, then `ClickupTasksStore` registers under `StoreBackend::ClickupTasks` in a non-generic dispatch registry, and `dispatch_for_type`'s closed generic match is replaced by a registry lookup.
- Given the refactor, then each existing backend (`GithubIssuesStore`/`GithubMilestonesStore`/`GithubProjectsStore`/`GitRefStore`) holds a boxed trait-object client internally instead of a generic client param, with no behavior change and existing tests passing unmodified.

## Non-functional

- New `TaskMap` at `.lazyspec/task-map.json` mirrors `IssueMap`'s shape; reuses `CacheLock` and `write_cache_file`/`write_cache_parent`/`write_cache_child` unchanged.
