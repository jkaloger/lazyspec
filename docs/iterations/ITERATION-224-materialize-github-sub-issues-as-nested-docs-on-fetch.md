---
title: Materialize GitHub sub-issues as nested docs on fetch
type: iteration
status: accepted
author: jkaloger
date: 2026-06-26
tags: []
related:
- implements: STORY-159
---## Context

Bug: GitHub native sub-issues pulled by `fetch` do not nest in TUI. They render flat.

Root cause (read path never ingests sub-issue parentage):
- `issue_cache.rs:262` `fetch_all` calls `gh.issue_list` (flat). No `subIssues` query.
- `issue_cache.rs:348` `parse_issue` hardcodes `related: vec![]`. Parentage dropped.
- `store_dispatch.rs:1554` `write_cache_file` + `find_cache_file:1588` (non-recursive) write every issue flat at `.lazyspec/cache/<type>/<ID>.md`. No `index.md` folders.
- TUI tree nests by filesystem `parent_of`/`children_of` (`loader.rs:140`, `app.rs:~1825`), built from subdir layout. Flat cache → empty `parent_of` → flat render.

STORY-159 built the push half (subdir children → `addSubIssue`, filesystem → GitHub). This is the symmetric pull half: reconstruct subdir cache layout from native sub-issues so the existing loader nests with zero TUI change.

`SUB_ISSUES_QUERY` already exists (`gh_subissue.rs:35`), used only on the write/reconcile path. Reuse it on fetch.

## Approach

Mechanism: materialize nested cache layout (inverse of STORY-159 push). No relation-edge nesting, no TUI change.

1. On `fetch_all`, after the flat `issue_list`, query each parent's native sub-issues (batch via `GhGraphql`, reuse `SUB_ISSUES_QUERY`). Build parent→children map keyed by issue node id, resolved to doc ids via `issue_map`.
2. For a parent with children: write cache as `<type>/<PARENT-folder>/index.md` (parent) + `<type>/<PARENT-folder>/NN-<child>.md` (children), `NN` from GitHub sub-issue order (`reprioritizeSubIssue` order, same key as loader sort, `loader.rs:100`). Childless issues stay flat.
3. `write_cache_file` / `find_cache_file` learn the nested layout (recurse or accept a subpath). Cache prune (`fetch_all` removed-set, `issue_cache.rs:321`) handles nested paths.
4. Github store load traverses cache subdirs through `load_subdirectory` (`loader.rs:105`) so `parent_of`/`children_of` populate for github types → TUI tree nests.

Same-store constraint holds (STORY-159): sub-issue endpoints issue-backed, parent+children same type/store.

## Acceptance Criteria

- Given a GitHub parent issue with native sub-issues and no local subdir, When `fetch`, Then cache materializes parent as `<folder>/index.md` and each sub-issue as `<folder>/NN-*.md`, ordered by GitHub sub-issue order.
- Given that fetched layout, When TUI lists docs, Then sub-issues render nested under the parent (`children_of` non-empty), matching subdir-authored nesting.
- Given a childless issue, When `fetch`, Then it stays flat at `<type>/<ID>.md` (no regression).
- Given a sub-issue removed on GitHub, When re-`fetch`, Then the child cache file is pruned and it no longer nests (re-parents flat or drops per remote).
- Given sub-issue parentage read fails (GraphQL error), When `fetch`, Then warn and fall back to flat cache (best-effort, mirrors schema-snapshot refresh `fetch.rs:101`); fetch does not abort.
- Given a doc with both native sub-issues and a semantic `implements` relation, When fetched, Then sub-issues drive nesting and `implements` stays comment-backed/`related` (unchanged).
- `--json` output of `fetch`/`status`/`show` unchanged in shape; nesting visible via existing parent/child fields.

## Out of Scope

- Cross-repo / cross-owner sub-issues (same-store by construction).
- Promoting semantic relations to nesting.
- `>8` nesting depth beyond GitHub limits; flat parent→child only (per RFC-050).
- Conflict detection on writes (last-write-wins, RFC-050).
