---
title: Every configured type's issues arrive complete, one request per page round
type: story
status: accepted
author: Jack Kaloger
date: 2026-08-06
tags: []
related:
- implements: RFC-065
- blocks: STORY-251
---

## Context

`discover_issues` costs one REST `issue_list` per type (`issue_cache.rs:720`), and for a type classified by a native issue type it is a true N+1: `search_issue_numbers_by_type` (`gh.rs:1350`) resolves numbers via GraphQL search, then one REST `issue_view` runs per number (`issue_cache.rs:731`). A 60-issue issue-type-classified type costs 61 requests — ~35s — to produce a list.

Selecting `issueType { name }` inline on `repository.issues` deletes the N+1 outright: the type is discovered by listing and filtering client-side, so `search_issue_numbers_by_type`, `ISSUE_TYPE_SEARCH_PAGE_SIZE` and `search_truncation_warning` all go, and a tag-plus-issue-type rule filters two fields of one result set instead of intersecting a REST list against a search. Each type becomes a field alias on the document the previous slice stood up, so T lists cost one request.

The mapping is nearly free. `GhIssue`'s serde names are already camelCase (`gh.rs:57-60`) because `gh issue list --json` returns camelCase. The only divergences are connection shapes — REST `labels: [{name}]` and `assignees: [{login}]` against GraphQL `{nodes: [...]}` — unwrapped by a parse helper, leaving `GhIssue` and every consumer of it untouched.

Pagination ships in this slice rather than after it, because it has to. GraphQL pages at 100 while `gh issue list --limit` currently reaches `FETCH_LIMIT = 500` (`issue_cache.rs:393`), so a first-page-only cut-over would be a visible regression from 500 issues to 100. Round 1 requests the first 100 for every type at once; types reporting `hasNextPage` have their `endCursor` composed into a second request alongside every other still-unfinished type. Requests total the largest type's page count, not the sum — one 300-issue type and nine small ones costs 3 requests, not 12. Fetch then becomes complete, so the 500 cap and its "there may be more" warning are removed rather than raised: the bound was an artifact of `gh issue list --limit`, never a requirement.

Value: a project with more than 500 issues of a type stops silently missing documents, discovery stops scaling with issue count, and both surfaces drop from T + T×N requests to one per page round.

## Acceptance Criteria

- **Given** 10 configured `github-issues` types exercising all four discovery rules — label, tag, native issue type, and tag plus issue type — each holding at most 100 issues
  **When** fetch runs
  **Then** one `gh` invocation serves every type's issue list, and a shim on `PATH` records zero REST `issue_list` and zero `issue_view` calls.

- **Given** a type classified by a native issue type holding 60 issues
  **When** fetch runs
  **Then** all 60 are discovered by filtering `issueType.name` on the single response, and `search_issue_numbers_by_type`, `ISSUE_TYPE_SEARCH_PAGE_SIZE` and `search_truncation_warning` no longer exist.

- **Given** a type whose rule is a tag plus a native issue type
  **When** fetch runs
  **Then** the intersection is computed over two fields of one result set — no second search, and no REST list to intersect against.

- **Given** one type with 300 issues and nine types with fewer than 100 each
  **When** fetch runs
  **Then** exactly 3 requests are made: rounds 2 and 3 compose only the still-unfinished type, and all 300 of its issues become documents.

- **Given** a type with more than 500 issues
  **When** fetch runs
  **Then** every issue becomes a document, no truncation warning is emitted, `FETCH_LIMIT` no longer exists, and README's fetch-warning list (line 517, "a truncated search") drops both retired warnings.

- **Given** identical repo state
  **When** fetch runs before and after this change
  **Then** the issue cache on disk is equivalent — same documents, node ids, numbers, titles, bodies, states, authors, assignees, labels, milestone numbers and timestamps — including the nested sub-issue layout and the `fetched`/`new`/`removed` counts `fetch --json` reports.

- **Given** a create, update or close mutation
  **When** it completes
  **Then** its read-back is still one `issue_view` (`store_dispatch.rs:859,907,1000`), unchanged — a single-doc read after a write is correctly one request.

- **Given** one type's issue subtree fails, from a permission error or an unknown label, while the others succeed
  **When** the round is parsed
  **Then** the successful types' issues land, the milestone and owner subtrees land, a warning names the failed type, and that type's prior cache is left intact rather than emptied.

- **Given** the TUI background poll rather than the CLI
  **When** it fires
  **Then** it makes the same request count with the same results, by construction through `sync_all`.

- **Given** any operation in this slice
  **When** its result is serialized
  **Then** it is available via `--json`.

## Scope

### In Scope

- Per-type field aliases on `repository.issues`, built from each type's discovery rule, selecting `issueType { name }`, `milestone { number }`, `labels`, `assignees` and `author { login }` inline.
- A parse helper unwrapping GraphQL `{nodes: [...]}` connections into `GhIssue`'s REST-shaped `labels` and `assignees`, leaving `GhIssue` and its consumers unchanged.
- Client-side classification for issue-type and tag-plus-issue-type rules; deleting `search_issue_numbers_by_type`, `ISSUE_TYPE_SEARCH_PAGE_SIZE` and `search_truncation_warning`.
- Composed cursor rounds: `pageInfo { hasNextPage endCursor }` per alias, `FetchSnapshot::next_pages`, and a loop that re-composes only unfinished types.
- Removing `FETCH_LIMIT` and its warning; updating README's fetch-warning list.
- `discover_issues` and `IssueCache::fetch_all`'s issue-list read served from the snapshot, with parse, layout, lock and diff logic unchanged.
- Fixture tests per rule shape, a multi-page fixture, a partial-failure fixture, and shim request-count assertions on both surfaces.
- ADR for "pagination composes across types per round".

### Out of Scope

- Sub-issues, blocked-by edges and project items. They stay on the existing `nodes(ids:)` batches in this slice and move in the next one.
- The node budget and nested-connection truncation warnings. Selections here are flat, ~31 nodes per issue, so 10 types sit far under the 500,000 cap; the arithmetic arrives with nested connections.
- Removing `GhIssueReader::issue_list` from the trait. Retained, just no longer called by fetch.
- `issue_view` on the mutation path, comment fetching, parallelism, cache-format changes.

### Split Point

Not at the pagination line — a first-page-only slice regresses from 500 issues to 100, so cursor rounds are part of the smallest non-regressive cut. The honest cut is by rule: label and tag types move to the composed alias first, issue-type-classified types stay on the search-plus-`issue_view` path, and the N+1 deletion follows as a second story.

