---
title: Issue-type GraphQL discovery for github-issues fetch
type: story
status: complete
author: jkaloger
date: 2026-07-02
tags: []
related:
- implements: RFC-055
---## Problem

`github-issues` fetch discovers candidate issues for a type with exactly one mechanism today: a server-side label filter. `IssueCache::refresh_stale` and `IssueCache::fetch_all` (src/engine/issue_cache.rs:131-245 and :302-448) each build `let label = type_label(&type_def.name); let labels = vec![label];` (issue_cache.rs:161-162 and :313-314) and pass it straight to `gh.issue_list(repo, &labels, ...)` (issue_cache.rs:179 and :317), which forwards to `gh issue list --label <value>` (src/engine/gh.rs:706, :718-720 in `GhIssueReader::issue_list`). That REST filter only understands labels.

RFC-055 (Story 1/STORY-191) adds `TypeDef.github_issue_tag`/`github_issue_type` (today's `TypeDef` has no such fields — src/engine/config.rs:240-266) as classification signals, and native GitHub Issue Type has no REST list filter at all. A type configured with `github_issue_type` needs a different discovery mechanism, and a type configured with *both* `github_issue_tag` and `github_issue_type` needs the two mechanisms' results combined by intersection, not union — per RFC-055's Design ("Discovery: label vs. issue-type search"), a union would wrongly surface issues that satisfy only one of the two configured signals.

## Goal

Add a GraphQL search-based discovery path for the native-issue-type signal, and wire it into both existing fetch entry points (`refresh_stale`, `fetch_all`) so each type's candidate issue set is resolved according to which signals it has configured:

- Neither signal set (today's default): unchanged, label-filtered `gh issue list --label lazyspec:{type}`.
- Only `github_issue_tag` set: unchanged mechanism, just a different label string (STORY-191/192's concern, not this story's).
- Only `github_issue_type` set: GraphQL search result set only, no REST label call.
- Both set: REST label-filtered result intersected with the GraphQL search result set, by issue number.

This story assumes STORY-191 (schema) has landed and STORY-192 (per-type match-rule plumbing) has threaded each type's resolved signal(s) down to `refresh_stale`/`fetch_all` in place of (or alongside) today's `known_types: &[String]`. This story does not itself add config fields or change classification (`extract_type_and_tags`/`parse_issue`'s label-matching logic) — it only adds the missing discovery mechanism and its intersection/union-avoidance behavior at the two `gh.issue_list` call sites.

## Design

### New GraphQL search query (gh.rs)

Add a query and parser next to the existing `ISSUE_TYPE_QUERY` / `parse_issue_type_name` pair (gh.rs:770-782), which already resolves one issue's native type over GraphQL and is the closest precedent in the file:

```rust
const ISSUE_TYPE_SEARCH_QUERY: &str =
    "query($searchQuery: String!) { search(query: $searchQuery, type: ISSUE, first: 100) { nodes { ... on Issue { number } } } }";

fn parse_search_issue_numbers(resp: &serde_json::Value) -> Vec<u64> {
    resp.pointer("/data/search/nodes")
        .and_then(|v| v.as_array())
        .map(|nodes| {
            nodes
                .iter()
                .filter_map(|n| n.pointer("/number").and_then(|v| v.as_u64()))
                .collect()
        })
        .unwrap_or_default()
}

pub fn search_issue_numbers_by_type(
    gh_graphql: &dyn GhGraphql,
    repo: &str,
    issue_type: &str,
) -> Result<Vec<u64>> {
    let search_query = format!("repo:{} is:issue type:\"{}\"", repo, issue_type);
    let resp = gh_graphql.graphql(
        ISSUE_TYPE_SEARCH_QUERY,
        &[("searchQuery", GqlVar::Str(search_query))],
    )?;
    Ok(parse_search_issue_numbers(&resp))
}
```

Notes on this design:

- The search string is passed as a GraphQL variable (`$searchQuery`), not interpolated into the query literal — the same pattern `ISSUE_TYPE_QUERY` already uses for `$owner`/`$name`/`$number` (gh.rs:753-756), and it sidesteps needing to escape the query body itself.
- No new trait method is needed on `GhGraphql` (gh.rs:425-452): the existing generic `graphql()` method and `build_graphql_args` (gh.rs:634-656) are reused as-is, the same way `issue_view` already builds an ad hoc query through them (gh.rs:750-757). `search_issue_numbers_by_type` is a free function, matching `parse_issue_type_name`'s shape, not a trait addition.
- `parse_search_issue_numbers` follows `parse_project_item_fields`'s (gh.rs:554-621) defensive style: missing/malformed nodes are skipped rather than erroring the whole parse.
- `first: 100` is a flat, unpaginated cap, matching the existing `FETCH_LIMIT = 500` flat cap already accepted for `fetch_all`'s REST call (issue_cache.rs:315, with an existing "there may be more" warning at :320-327 when the cap is hit). Pagination beyond one page is out of scope here (see Non-goals) — this story should emit an equivalent warning when a search returns exactly 100 numbers.
- The `type:"..."` GitHub search qualifier does not itself escape an embedded `"` in the configured issue-type name; a type name containing a double quote will produce a malformed search query. Flagged as a known edge case, not solved by this story (native GitHub Issue Type names are short controlled-vocabulary strings in practice; STORY-191 is the right place to decide whether to validate this at config time).

### Wiring into the two discovery call sites (issue_cache.rs)

Both `refresh_stale` (src/engine/issue_cache.rs:131-193) and `fetch_all` (:302-317) already receive `gh_graphql: &dyn GhGraphql` as a parameter (:136 and :307 respectively) — it is already threaded through for the native-field schema-snapshot refresh (`refresh_schema_snapshot`, :250-271), so this story needs no new plumbing to reach a GraphQL client from either call site; it only needs the per-type signal values that STORY-192 threads in.

Each site's `let label = type_label(&type_def.name); let labels = vec![label];` block (issue_cache.rs:161-162, :313-314) becomes a branch on the type's resolved discovery signal(s):

- **Label/tag only** (today's default, or `github_issue_tag` alone): unchanged — `gh.issue_list(repo, &labels, &fields, ...)` as today, just resolving the label string from whatever STORY-191/192 hand down instead of always `type_label(&type_def.name)`.
- **`github_issue_type` only**: call `search_issue_numbers_by_type(gh_graphql, repo, issue_type)`, then resolve each returned number to a full `GhIssue` via the existing `GhIssueReader::issue_view` (gh.rs:240, implemented :732-761) — `issue_view` already fetches full REST fields *and* resolves native `issue_type` over GraphQL in one call, so no new "fetch full issue by number" path is needed. `gh.issue_list` is not called at all for this type.
- **Both set**: call the existing REST `gh.issue_list(repo, &[tag], &fields, ...)` (full `GhIssue` objects, unchanged) *and* `search_issue_numbers_by_type` (issue numbers only), then filter the REST result down to issues whose `number` is in the search result set. This reuses the REST call's already-fetched full issue data instead of also calling `issue_view` per number, and is a true set intersection — an issue returned by only one of the two calls is dropped, matching RFC-055's Design explicitly (AND semantics, not union).

Both call sites should share one resolution helper (e.g. `fn discover_issues(gh, gh_graphql, repo, rule, fields, limit) -> Result<Vec<GhIssue>>` in issue_cache.rs) rather than duplicating the three-way branch independently in `refresh_stale` and `fetch_all`, since today those two functions already duplicate the `let label = ...; let labels = ...;` two-liner and diverge only in `fields`/`limit`.

### Cost profile

- Label-only and tag-only types: no change in call count (one REST call, as today).
- Both-signals types: one REST call plus one GraphQL search call, per fetch — the cost RFC-055's Risks section already anticipates ("cost scales with the number of types configuring `github_issue_type`, not with total repo issue count").
- Issue-type-only types: one GraphQL search call plus one `issue_view` GraphQL+REST round trip *per matched issue*. This N+1 cost is not called out in RFC-055's Risks section (which focused on the both-signals case) and is worth flagging here: it is proportional to the number of matched issues, not just the number of issue-type-configured types. Accepted for this story since no bulk "fetch full issues by number list" GraphQL query exists yet in this codebase; revisit if it proves too slow in practice.

## Non-goals

- Adding `github_issue_tag`/`github_issue_type` to `TypeDef` or any config validation — STORY-191.
- Changing `extract_type_and_tags`/`parse_issue`'s classification logic, or threading per-type match rules through `IssueContext` builders — STORY-192. This story consumes whatever signal values STORY-192's plumbing supplies; it does not itself decide how a type's rule is resolved from config.
- Dual materialization (two docs from one issue matching two types) — STORY-194.
- `create` pushing `github_issue_type` onto newly created issues via `push_issue_type` — STORY-195.
- README documentation of the new fields — STORY-196.
- Pagination past the first 100 GraphQL search results (or past the existing 500 REST `FETCH_LIMIT`) — a warning on truncation is in scope; fetching further pages is not.
- Any change to the write side (`push_issue_type`, `issue_create`'s label loop at gh.rs:795-798) — this story is discovery/read-side only.

## Acceptance criteria

- **Given** a type with only `github_issue_type` configured, **when** fetch runs (`refresh_stale` or `fetch_all`), **then** no `gh issue list --label` call is made for that type, and the candidate issue set is exactly the numbers returned by `search_issue_numbers_by_type`, each resolved to a full issue via `issue_view`.
- **Given** a type with only `github_issue_tag` set (or neither field set, today's default), **when** fetch runs, **then** discovery behavior is byte-for-byte unchanged from today: a single REST `gh.issue_list` call with that type's resolved label, no GraphQL search call.
- **Given** a type with both `github_issue_tag` and `github_issue_type` configured, **when** fetch runs, **then** both the REST label call and the GraphQL search call are made, and the resulting candidate set is their intersection by issue number — an issue present in only one of the two result sets is excluded, never included via a union.
- **Given** a GraphQL search response shaped like `{"data":{"search":{"nodes":[{"number":42},{"number":7}]}}}`, **when** `parse_search_issue_numbers` parses it, **then** it returns `[42, 7]`; given malformed or missing nodes, it returns an empty list rather than erroring.
- **Given** a GraphQL search returns exactly 100 issue numbers, **when** fetch runs for that type, **then** a `RefreshWarning`/`FetchResult` warning is surfaced noting the result may be truncated, mirroring the existing `FETCH_LIMIT`-hit warning (issue_cache.rs:320-327).
- **Given** `MockGhClient`'s existing `graphql_responses`/`graphql_calls` test seam (gh.rs:1167-1168, :1676-1691) which already records calls and returns queued canned responses with no mock changes needed, **when** unit tests exercise `search_issue_numbers_by_type` and the both-signals intersection path, **then** they can do so entirely through that existing seam.
