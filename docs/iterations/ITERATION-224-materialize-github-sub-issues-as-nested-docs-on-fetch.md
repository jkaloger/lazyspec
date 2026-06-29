---
title: Materialize GitHub sub-issues as nested docs on fetch
type: iteration
status: complete
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

Investigation note (pre-execution code map): loader ALREADY recurses for github cache (`store.rs:60` points github types at `.lazyspec/cache/<type>`, `load_type_directory` → `load_subdirectory` descends). Approach step 4 needs no loader change. Real work = make `fetch_all` WRITE nested layout + make prune/cache helpers nested-aware. Seam: `extract_id` (`store.rs:440`) does not strip `NN-` prefix from child stem → child filename scheme must yield correct doc id via existing loader (loader sets `meta.id = extract_id(path)`, overriding frontmatter id). No reverse `node_id → doc_id` lookup exists in `IssueMap`; build local map in `fetch_all` (every fetched issue has `issue.id` node id + doc id).

## Task Breakdown

### TASK-1: Nested cache layout in read/write helpers

Goal: cache helpers handle `<type>/<PARENT>/index.md` + `<type>/<PARENT>/NN-<child>.md`, not just flat `<type>/<ID>.md`.

Scope:
- `store_dispatch.rs:1543` `write_cache_file` + `find_cache_file:1588`: accept nested layout. Add variant/param for subpath OR recurse. Parent of children → `<PARENT-folder>/index.md`. Child → `<PARENT-folder>/NN-<child-id>.md`. `NN` zero-padded wide enough lexicographic-sort = numeric-sort (≥2 digit; 3 if >99 children).
- Child filename scheme must make loader resolve correct doc id. `extract_id` (`store.rs:440`) uses stem; does NOT strip `NN-` prefix. Either: strip leading `NN-` numeric prefix in `extract_id` for nested children, OR scheme where stem after prefix = real id. MUST match what filesystem-authored subdir children produce so cache children + source children resolve identical ids. Verify against existing subdir child convention before choosing.
- `find_cache_file` recursive: find child inside parent folder.
- `issue_cache.rs:241` `list_cached` recursive: descend one level into parent folders so prune sees nested children. `issue_cache.rs:97` `remove`: delete nested child file; remove parent `index.md` + empty folder when parent itself removed.

TDD: unit tests round-trip write→find→list→remove on nested layout. Folder name = parent doc id. Child stem resolves to child doc id via loader.

AC: nested write produces `<PARENT>/index.md` + `<PARENT>/NN-*.md`; `find_cache_file`/`list_cached` see nested; `remove` prunes nested; childless still flat at `<type>/<ID>.md` (no regression).

### TASK-2: Fetch native sub-issues, build parent→children map

Goal: `fetch_all` learns remote parentage, best-effort.

Scope:
- `issue_cache.rs:262` `fetch_all`: after flat `issue_list` pass, build local `node_id → doc_id` map from fetched issues (`issue.id` node id, `id` doc id).
- For each fetched parent, query `subIssues` via `gh_graphql` (reuse `fetch_remote_sub_issue_nodes`, `gh_subissue.rs:145`; reuse `SUB_ISSUES_QUERY:35`). Ordered child node ids → resolve to doc ids via local map. Build ordered parent→children map. `NN` order = array index from `subIssues.nodes`.
- Best-effort (AC mirror `fetch.rs:101` schema-snapshot): graphql error → `eprintln!` warn + fall back to flat for that fetch; do NOT abort `fetch_all`.

TDD: extend `issue_cache.rs` `fetch_all` tests using `MockReader.with_graphql_responses` (FIFO queue, `gh_subissue.rs` test helper `sub_issues(&[...])` for `subIssues.nodes` shape). Assert parent→children map built; graphql-error case falls back flat without error.

AC: parent with native sub-issues → ordered child map keyed by doc id; graphql failure → warn + flat fallback, fetch completes.

### TASK-3: Materialize nested cache + prune on fetch

Goal: `fetch_all` writes nested layout from TASK-2 map using TASK-1 helpers; prune handles nested.

Scope:
- In `fetch_all` write loop: parent-with-children → write parent `index.md` + children `NN-<id>.md` (nested). Childless → flat (unchanged path).
- Prune (`issue_cache.rs:321` removed-set) uses recursive `list_cached` (TASK-1): child removed on GitHub re-fetch → child cache file pruned, re-parents flat or drops per remote.
- `issue_map.insert` for children unchanged (number/node_id/updated_at).

TDD: `fetch_all` integration over `Store::load` asserts `children_of(parent)` non-empty matching subdir-authored nesting; childless flat; re-fetch with child removed prunes child + nesting gone.

AC (iteration ACs 1,2,3,4): fetch materializes `<folder>/index.md`+`<folder>/NN-*` ordered by GitHub order; TUI loader nests (`children_of` non-empty); childless flat; removed sub-issue pruned + un-nested on re-fetch.

### TASK-4: End-to-end integration + json-shape guard

Goal: prove full `fetch` path nests + `--json` shape unchanged.

Scope:
- `tests/integration/fetch_prune_test.rs` style: run `cli::fetch::run` with gh mock exposing flat issues + sub-issue graphql. Assert cache nested layout on disk + `Store::load` nesting.
- Guard: `fetch`/`status`/`show` `--json` shape unchanged (nesting visible only via existing parent/child fields). Semantic `implements` relation coexists: sub-issues drive nesting, `implements` stays comment-backed/`related` unchanged.

TDD: integration test asserts nested cache + json keys identical shape to flat (no new top-level fields). Doc with both native sub-issues + `implements` → nesting from sub-issues, `related` keeps `implements`.

AC (iteration ACs 6,7): both-relation doc nests via sub-issues + keeps `implements` in `related`; `--json` shape of fetch/status/show unchanged.

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
