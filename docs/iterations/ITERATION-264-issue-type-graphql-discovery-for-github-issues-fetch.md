---
title: Issue-type GraphQL discovery for github-issues fetch
type: iteration
status: complete
author: jkaloger
date: 2026-07-03
tags: []
related:
- implements: STORY-193
---

## Objective

GraphQL search discovery for `github_issue_type`-configured types; both-signals case intersects REST label results with GraphQL search results by issue number (AND, not union).

## Context

- Story: STORY-193 (assumes STORY-191 schema + STORY-192 `TypeMatchRule` plumbing landed -- ITERATION-262, ITERATION-263)
- Design (exact query, `parse_search_issue_numbers`, `search_issue_numbers_by_type` signature, `discover_issues` helper shape, cost profile): STORY-193 body verbatim -- don't re-derive.
- Touch: src/engine/gh.rs (new `ISSUE_TYPE_SEARCH_QUERY`, `parse_search_issue_numbers`, `search_issue_numbers_by_type`, next to `ISSUE_TYPE_QUERY`/`parse_issue_type_name` at gh.rs:770-782), src/engine/issue_cache.rs (`refresh_stale`:131-193, `fetch_all`:302-317 -- shared `discover_issues` helper).

## Satisfies

STORY-193 AC1-AC6 (all -- one discovery-branch change shared by both fetch entry points, not separable).

## Tasks

1. gh.rs: add `ISSUE_TYPE_SEARCH_QUERY` const, `parse_search_issue_numbers` (defensive parse, empty on malformed), `search_issue_numbers_by_type` (free fn, reuses existing `GhGraphql::graphql` + `build_graphql_args`, `$searchQuery` variable per `ISSUE_TYPE_QUERY` pattern).
2. issue_cache.rs: extract shared `discover_issues(gh, gh_graphql, repo, rule: &TypeMatchRule, fields, limit) -> Result<Vec<GhIssue>>` covering the three branches: label/tag-only (unchanged REST `issue_list`), issue-type-only (search + resolve each number via `issue_view`, no `issue_list` call), both-set (REST + search, intersect by issue number, drop non-intersecting).
3. Wire `refresh_stale` and `fetch_all` to call `discover_issues` instead of their current inline `let label = ...; let labels = vec![label];` block.
4. Truncation warning: when GraphQL search returns exactly 100 numbers, surface a `RefreshWarning`/`FetchResult` warning mirroring the existing `FETCH_LIMIT`-hit warning (issue_cache.rs:320-327).
5. Unit tests via `MockGhClient`'s existing `graphql_responses`/`graphql_calls` seam (gh.rs:1167-1168,:1676-1691): `parse_search_issue_numbers` on well-formed and malformed responses; both-signals intersection drops an issue present in only one result set; issue-type-only path makes zero `issue_list` calls.

## Out of scope

- Config fields, classification logic -- STORY-191/192.
- Dual materialization -- STORY-194.
- `create` write-side push -- STORY-195.
- README -- STORY-196.
- Pagination past 100 (search) / 500 (REST `FETCH_LIMIT`) -- warn only, no further pages.
- Escaping `"` in issue-type names for the search qualifier -- known edge case, not solved here.

## Principles/conventions

Engine-only (CONVENTION.md L3) -- no new `GhGraphql` trait method, reuses existing `graphql()`/`build_graphql_args`, matching `issue_view`'s existing ad hoc query pattern.

## Verification

- Only `github_issue_type` set: zero `gh issue list --label` calls; candidate set is exactly the search result, each resolved via `issue_view`.
- Only `github_issue_tag` set (or neither): discovery byte-for-byte unchanged from today -- one REST call, no GraphQL search.
- Both set: both calls made, result is issue-number intersection, not union.
- `parse_search_issue_numbers` on `{"data":{"search":{"nodes":[{"number":42},{"number":7}]}}}` -> `[42, 7]`; malformed/missing nodes -> `[]`.
- Search returning exactly 100 numbers -> truncation warning surfaced.

