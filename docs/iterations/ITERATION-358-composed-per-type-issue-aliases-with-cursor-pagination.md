---
title: Composed per-type issue aliases with cursor pagination
type: iteration
status: in-progress
author: Jack Kaloger
date: 2026-08-06
tags: []
related:
- implements: STORY-250
- blocks: ITERATION-359
---

## Objective

Each `github-issues` type becomes a field alias on `repository.issues` in the composed doc, with `issueType { name }` selected inline. Deletes the issue-type N+1 outright. Cursor rounds re-compose only unfinished types, so fetch becomes complete and `FETCH_LIMIT` goes away rather than up.

## Context

- Story: STORY-250. Design: RFC-065 §"Composed document, built from config" (alias shape, node selection, `GhIssue` mapping) and §"Pagination: composed cursor rounds". Don't re-derive the query or the cost model.
- Builds on: `gh_fetch::fetch_round` + `FetchSnapshot` from STORY-249's iterations.
- Touch: `src/engine/gh_fetch.rs` (per-type alias builder, node -> `GhIssue` parse helper, `next_pages`), `src/engine/issue_cache.rs` (`fetch_all`:381 -- `FETCH_LIMIT`:393, truncation warning :402-408, `discover_issues`:709, `Discovery`, `search_truncation_warning`:758, `refresh_stale`:149 at :206/:278), `src/engine/gh.rs` (`search_issue_numbers_by_type`:1350, `ISSUE_TYPE_SEARCH_PAGE_SIZE`:1325, `ISSUE_TYPE_SEARCH_QUERY`, `parse_search_issue_numbers` -- all delete; `GhIssue`:41 and `GhAssignee`/`GhLabel` unchanged), `src/engine/sync.rs` (round loop), `README.md:517`.
- `GhIssue.issue_type` is `#[serde(default, skip)]` (gh.rs:66-68) -- set it imperatively from `issueType.name`, serde will not.

## Satisfies

STORY-250 AC1-AC10 (all). Single code path: one alias builder, one parse helper, four rule shapes that differ only by a filter predicate, and a cursor loop the story's own Split Point forbids deferring.

## Tasks

1. Builder: per type index `i`, `t<i>: issues(first: 100, states: [OPEN, CLOSED], labels: [...], after: $c<i>) { pageInfo { hasNextPage endCursor } nodes { ... } }`. Node selection per RFC-065: `id number url title body state updatedAt createdAt author { login } issueType { name } milestone { number } labels(first: 20) { nodes { name } } assignees(first: 10) { nodes { login } }`.
2. `labels:` arg from `TypeMatchRule`: `tag` if set, else `label`. A rule with only `issue_type` set emits no `labels:` filter -- it lists and filters client-side. That widens that alias's result set; accepted, it is what deletes the N+1.
3. Parse helper `gh_fetch::issue_from_node`: unwrap `labels { nodes }` and `assignees { nodes }` into `GhIssue`'s REST-shaped `Vec<GhLabel>`/`Vec<GhAssignee>`; set `issue_type` from `issueType.name`. `GhIssue` and every consumer of it stay untouched -- the divergence is absorbed here, not in the struct.
4. Client-side classification against `TypeMatchRule`, replacing `discover_issues`'s three branches: label/tag already filtered by the `labels:` arg; issue-type-only keeps nodes whose `issue_type` matches; tag-plus-issue-type ANDs both fields of the one result set -- no second search, no intersection against a REST list.
5. Cursor rounds: `FetchSnapshot::next_pages` (type name -> `endCursor`) populated from each alias's `pageInfo`. Loop in `sync.rs` re-composes only aliases still reporting `hasNextPage`, merging per-type issue vectors into one snapshot. Requests = largest type's page count, not the sum.
6. `IssueCache::fetch_all` takes its issue list from the snapshot. Parse, layout, lock and diff logic unchanged. Delete `FETCH_LIMIT` and its "there may be more" warning.
7. `discover_issues` also serves `refresh_stale` (issue_cache.rs:206), which runs from `main.rs:893`, NOT through `sync_all`. Route `refresh_stale` through a single-type `fetch_round` so the search machinery can actually be deleted. Its stale-cache fallback on API failure (:207-215) and its early returns for empty/fresh caches must survive unchanged. If this turns out to need a different shape, stop and flag -- do not leave `refresh_stale` calling a deleted fn.
8. Delete `discover_issues`, `Discovery`, `search_truncation_warning`, `search_issue_numbers_by_type`, `ISSUE_TYPE_SEARCH_QUERY`, `parse_search_issue_numbers`, `ISSUE_TYPE_SEARCH_PAGE_SIZE`, and their tests (issue_cache.rs:1831, :1893, :1949, :1993, :2656; gh.rs:3453).
9. `README.md:517`: drop "a truncated search" from the `fetch --json` warning list. Both retired warnings go.
10. Tests:
    - Fixture per rule shape -- label, tag, native issue type, tag-plus-issue-type -- asserting the alias's `labels:` arg and the resulting `Vec<GhIssue>`.
    - Multi-page fixture: one 300-issue alias plus nine short ones -> exactly 3 rounds, rounds 2 and 3 compose only the unfinished alias, all 300 become documents.
    - Partial-failure fixture: one alias errored -> other types' issues land, that type's prior cache left intact rather than emptied, warning names the type.
    - `GhIssueReader` seam: make the double panic in `issue_list`/`issue_view` for the fetch tests, so zero-REST is asserted at the trait rather than by counting strings.
    - `gh` shim request-count assertions on both the CLI and the TUI poll.
    - Existing doubles (`MockReader` issue_cache.rs:1299, `StubGh` cli/fetch.rs:998, `NoopGh`/`NestingGh` in tests/integration) now answer the composed issue aliases from their `graphql()` impl.
11. ADR via `lazyspec create adr`, linked `implements` RFC-065: "Pagination composes across types per round" (RFC-065 decision 3).

## Out of scope

- Sub-issues, blocked-by edges, project items -- STORY-251. They stay on the existing `nodes(ids:)` batches (`fetch_subissue_parentage`, `list_blocked_by_batch`, `reconcile_project_fields_into_cache`) this slice.
- Node budget and nested-connection truncation. Selections here are flat, ~31 nodes per issue; 10 types sit far under 500,000.
- `issue_view` on the mutation read-back path (store_dispatch.rs:859, :907, :1000) -- correctly one request after a write.
- Removing `GhIssueReader::issue_list` from the trait. Retained, uncalled by fetch.
- Comment fetching, parallelism, cache-format changes, `IssueMap`/lock/nested-layout shapes.

## Principles/conventions

CONVENTION.md 2 (`--json`), 3 (engine; CLI and TUI unchanged, neither depends on the other), 4 (`GhGraphql` remains the only fake seam -- doubles answer a composed query, they do not get a new method), 6 (the parse helper is one fn, not a trait).

Early returns in the rule-filter branches. Exhaustive `match` on the `(tag, issue_type)` pair as `discover_issues` does today -- keep that, drop the bodies.

## Verification

- 10 types across all four rules, <=100 issues each: one invocation serves every list; zero `issue_list`, zero `issue_view`.
- Issue-type-classified type with 60 issues: all 60 discovered by filtering `issueType.name`; `grep -rn "search_issue_numbers_by_type\|ISSUE_TYPE_SEARCH_PAGE_SIZE\|search_truncation_warning\|FETCH_LIMIT" src` returns nothing.
- Tag-plus-issue-type: AND over two fields of one result set.
- 300-issue type + nine small: exactly 3 requests, 300 docs.
- >500-issue type: every issue becomes a document, no truncation warning.
- Cache parity on identical repo state: same documents, node ids, numbers, titles, bodies, states, authors, assignees, labels, milestone numbers, timestamps, nested sub-issue layout, and the same `fetched`/`new`/`removed` from `fetch --json`.
- Mutation read-back still one `issue_view`.
- One type's subtree failing leaves the others' issues and that type's prior cache intact.
- TUI poll: same count, same results.

