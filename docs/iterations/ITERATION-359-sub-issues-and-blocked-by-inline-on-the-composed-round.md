---
title: Sub-issues and blocked-by inline on the composed round
type: iteration
status: complete
author: Jack Kaloger
date: 2026-08-06
tags: []
related:
- implements: STORY-251
- blocks: ITERATION-360
---

## Objective

`subIssues` and `blockedBy` selected inline on each type's alias -> sub-issue parentage and dependency edges cost zero additional requests. Retires `fetch_subissue_parentage`'s and `list_blocked_by_batch`'s `nodes(ids:)` round trips.

## Context

- Story: STORY-251. Design: RFC-065 §"What the syncers consume" and §"Node budget". The batch scaffolding being deleted exists only because those reads were expensive; inline selection removes the reason.
- Builds on: STORY-250's per-type `t<i>` aliases.
- Touch: `src/engine/gh_fetch.rs` (connection selections, caps as consts, parsers), `src/engine/issue_cache.rs` (`fetch_all`:381 dependency block ~:495-540, `fetch_subissue_parentage`:777, `fetch_subissue_parent_numbers`:~815, `ParentageMap`), `src/engine/gh.rs` (`GhIssueDependencyApi::list_blocked_by_batch` default :487 + `GhCli` override :1611), `src/engine/gh_subissue.rs` (`fetch_sub_issue_nodes_batch`:187, `fetch_sub_issue_parent_numbers_batch`:224, `SUB_ISSUE_BATCH_MAX`:54).

## Satisfies

STORY-251 AC7, AC8, AC9; AC1 for the dependency and sub-issue reads; AC2 for the nested layout and the `blocks`/`blocked-by` relations; AC4 for the `subIssues`/`blockedBy` caps. `projectItems`, the budget arithmetic and AC3/AC5/AC6 -> next iteration.

## Tasks

1. Builder: `subIssues(first: 50) { pageInfo { hasNextPage } nodes { id number } }` and `blockedBy(first: 50) { pageInfo { hasNextPage } nodes { number } }` inline on each `t<i>` alias, the `first:` arguments rendered from the caps rather than typed twice. The caps live on a `Connection` enum -- `Connection::SubIssues.cap() = 50`, `Connection::BlockedBy.cap() = 50` -- whose variants also supply the GraphQL field name the truncation warning quotes.
2. Populate `FetchSnapshot::sub_issues` (parent node id -> ordered child node ids, server order) and `blocked_by` (issue number -> blocking numbers).
3. `fetch_all`: build the `ParentageMap` from `snapshot.sub_issues` in place of `fetch_subissue_parentage`. Resolution semantics unchanged -- child node ids absent from `node_to_doc` are dropped, parents with no resolvable child are omitted. `inject_subissue_relation` and the nesting-vs-relation split (ITERATION-224) untouched.
4. `fetch_subissue_parent_numbers` (flat-doc relation injection) reads the same map inverted -- child node id -> parent number. One source, not a second read.
5. Dependency block in `fetch_all`: `snapshot.blocked_by` in place of `gh_dependency.list_blocked_by_batch`. The `number -> doc id` batch map, the cross-type fallback through `IssueMap`, and the "forward `blocks` is derived virtually, never stored" posture all unchanged.
6. `hasNextPage == true` on either connection -> `RefreshWarning` naming the document id and the connection. Truncation is reported, never silent.
7. Delete `GhIssueDependencyApi::list_blocked_by_batch` (default + `GhCli` override) and, once unused, `gh_subissue::fetch_sub_issue_nodes_batch`, `fetch_sub_issue_parent_numbers_batch`, `SUB_ISSUE_BATCH_MAX` and their queries/parsers. `GhIssueDependencyApi::list_blocked_by` goes with it -- it has had no production caller since ee21fbe. Keep `gh_subissue::fetch_remote_sub_issue_nodes` and `reconcile_subissues` -- write path. `gh::GH_NODES_BATCH_MAX` survives this slice; `project_items_batch` is still its user.
8. No budget arithmetic. `subIssues(50) + blockedBy(50)` is ~100 nodes per issue; 16 types x 100 issues stays well under 500,000. The arithmetic arrives with `projectItems`, per the story's Split Point.
9. Tests:
   - Fixtures: nested sub-issue layout, flat-type parent-relation injection, blocked-by edges resolving same-type and cross-type.
   - Truncation fixture: 51 sub-issues -> `hasNextPage: true` -> one warning naming doc + connection.
   - Parity: nested layout and `blocks`/`blocked-by` relations equivalent to the batch path's output on identical state.
   - Doubles now answering the composed connections instead of the batch queries: `ParentGraphql` (issue_cache.rs:3838), `NestingReader` (:4095), `ParentReader` (:4453), `MockGhDependencyClient` (gh.rs:2329), `NestingGh` (tests/integration/fetch_nested_subissues_test.rs). `GhGraphql`/`GhIssueDependencyApi` stay the seams -- no new methods.
   - `gh` shim: zero `nodes(ids:)` invocations for parentage and dependency reads, on both surfaces.

## Out of scope

- `projectItems`, `fieldValues`, node-budget arithmetic, type splitting across `ceil(T/12)` requests, board-derived status -- next iteration (STORY-251 AC3, AC5, AC6, and AC1's `project_items_batch`/`PrefetchedProjectItems`/`reconcile_target_node_id` deletions).
- A per-issue follow-up query for overflowing connections. Recorded in RFC-065's risks, deliberately not built -- truncation warns instead.
- Raising the caps or making them configurable.
- `issue_view` read-back, comment fetching, parallelism, cache-format changes, `IssueMap`/lock/nested-layout shapes.

## Principles/conventions

CONVENTION.md 2 (warnings via `--json` per type), 3 (engine only), 4 (existing trait seams; doubles grow to answer the composed doc), 6 (delete the batch indirection once its second use is gone).

Early returns in the connection parsers. Where a comment about why a child is dropped is tempting, name the fn instead.

## Verification

- 10 types with parentage, dependencies: enrichment adds zero requests to the round; shim records no `nodes(ids:)`.
- `grep -rn "list_blocked_by_batch\|SUB_ISSUE_BATCH_MAX\|fetch_sub_issue_nodes_batch" src tests` returns nothing.
- Nested sub-issue layout byte-equivalent to the batch path on identical state; `blocks`/`blocked-by` relations identical.
- Issue with 51 sub-issues: 50 land, one warning names the doc and `subIssues`.
- Every dependency edge in the cache comes from the round's inline `blockedBy` selection: no `nodes(ids:)`, no per-issue dependency read on any path.
- TUI poll: same request count, same warnings.


