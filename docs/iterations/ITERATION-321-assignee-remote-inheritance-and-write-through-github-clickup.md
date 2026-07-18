---
title: Assignee remote inheritance and write-through (github, clickup)
type: iteration
status: complete
author: unknown
date: 2026-07-18
tags: []
related:
- implements: STORY-222
---

## Objective

github-issues docs inherit issue assignee on sync; clickup docs inherit task assignee on sync. Setting assignee via lazyspec on a remote-backed doc writes through to the remote. Remote is source of truth for those stores.

## Satisfies

STORY-222 AC3, AC4. Depends on core slice (assignee field must exist) — this iteration `blocked-by` it.

## Context

- Story + ACs: STORY-222.
- Model: assignee is a NATIVE remote field, parallel to github `milestone`/`labels` and clickup native `status`/`tags` — inherited on fetch, written through via dedicated remote mutation, NOT stored in the body round-trip HTML comment.
- `assignees` appears NOWHERE in codebase today — new field on every struct + mapping below.
- Fetch (remote->local) = `src/engine/sync.rs` `TypeSync::sync` / `sync_all` (L277) -> per-backend cache refresh. Write (local->remote) = `DocumentStore::update` in `store_dispatch.rs`.
- GitHub touch: `src/engine/gh.rs` — `GhIssue` (L29-63) add `assignees: Vec<GhAssignee>`, new `GhAssignee{login}` mirroring `GhAuthor`; `GhIssueWriter` trait (L253-279) add assignee mutation (`gh issue edit --add-assignee/--remove-assignee`). `src/engine/issue_cache.rs` — requested json fields vec (L184-194) add `"assignees"`; `parse_issue` (L693-756) inherit into `DocMeta.assignee`. `src/engine/issue_body.rs` — `IssueContext` (L43-59) thread native assignee; `serialize` (L64-68) keep assignee OUT of comment (native, like title/labels/state). `src/engine/store_dispatch.rs` — `GhIssueStore::update` match (L1321-1335) add `"assignee"` arm calling writer + resync.
- Write-through precedent: milestone native-edge. `gh.rs` `issue_set_milestone` (L310-317), `build_set_milestone_args` (L202-217); `resync_after_native_edge` (store_dispatch.rs L531-559); status open/close mapping (L1389-1400). Copy this template (native PATCH separate from body round-trip), NOT `push_cache` body path.
- ClickUp touch: `src/engine/clickup.rs` — `ClickupTask` (L216-246) add `assignees: Vec<ClickupAssignee>` mirroring `ClickupCreator`; `TaskUpdate` (L339-355) add `assignees {add, rem}`. `src/engine/clickup_cache.rs` — `task_to_doc` (L226-279, near L269) inherit assignee; `build_task_update` (L440-454) add assignee case. `src/engine/store_dispatch.rs` — `ClickupStore::update` (L251-293) push via `update_task`.

## Tasks

1. Test-first (github sync, `issue_cache` tests): fixture `GhIssue` with `assignees` => `DocMeta.assignee == Some(first login)` (multi maps to first — out of scope). No assignees => `None`.
2. `gh.rs`: add `GhAssignee{login}` + `assignees` on `GhIssue` (L29). `issue_cache.rs`: add `"assignees"` to fields vec (L184).
3. `issue_cache.rs` `parse_issue` (L693): set `DocMeta.assignee` from `issue.assignees` first entry (AC3). Thread via `IssueContext` (`issue_body.rs` L43); keep out of `serialize` comment (L64) — native.
4. GitHub write-through: add `GhIssueWriter` method (`gh.rs` L253), e.g. `issue_set_assignee(number, add, remove)` via `gh issue edit`; branch in `GhIssueStore::update` match (`store_dispatch.rs` L1321) on `"assignee"` => diff vs remote, call writer, `resync_after_native_edge` (L531). Follow milestone precedent (AC4).
5. Test-first (clickup sync): fixture `ClickupTask` with `assignees` => `DocMeta.assignee == Some(first username)`.
6. `clickup.rs`: `ClickupAssignee` + `assignees` on `ClickupTask` (L216); `assignees {add, rem}` on `TaskUpdate` (L339).
7. `clickup_cache.rs`: `task_to_doc` (L226/L269) inherit assignee (AC3); `build_task_update` (L440) add assignee case => `assignees {add, rem}` (AC4). `ClickupStore::update` (L251) push via `update_task`.
8. Test-first (write-through, mocked `GhIssueWriter`/`ClickupClient`): `update <gh-doc> --assignee X` fires assignee edit; `update <clickup-doc> --assignee X` builds assignees payload.

## Out of scope

- AC1/AC2/AC6 field + filesystem/git-ref settability + JSON → core slice (blocking dep).
- AC5 display surfaces → surfaces slice.
- Multi-assignee: remote multi maps to first only.
- Identity mapping across github/clickup/git identities — STORY-222 out-of-scope.

## Principles / conventions

- CLAUDE.md: account for engine change across tui/web/cli; `--json` output.
- Mirror milestone native-field precedent: inherit-on-fetch + dedicated write-through, NOT body-comment round-trip.
- Remote is source of truth for github/clickup assignee (AC3): sync overwrites local.

## Verification

- github doc after `sync`: assignee == issue's assignee (AC3).
- `update <gh-id> --assignee carol`: `gh issue edit` assignee mutation fired; re-sync shows carol (AC4).
- clickup doc: inherit on sync (AC3); `update` writes `assignees` add/rem payload via `update_task` (AC4).

