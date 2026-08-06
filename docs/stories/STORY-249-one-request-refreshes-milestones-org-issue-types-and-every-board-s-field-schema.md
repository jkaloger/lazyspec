---
title: One request refreshes milestones, org issue types and every board's field schema
type: story
status: complete
author: Jack Kaloger
date: 2026-08-06
tags: []
related:
- implements: RFC-065
- blocks: STORY-250
---

## Context

A fetch on a 10-type project with one authority board spends 31 of its ~51 requests on data that has nothing to do with issues. One REST `milestone_list` (`milestone_cache.rs:22`). Then `refresh_schema_snapshot` (`issue_cache.rs:682`) runs **per type**, re-fetching identical org-level issue types every time — T requests for one answer. Then `fetch_project_fields` (`gh_schema.rs:265`) runs per type per board, and `try_org_then_user` (`gh_schema.rs:227`) discovers the account kind by firing the org query and letting it fail, so every user-owned repo — `jkaloger/lazyspec` included — pays a second request to learn what one `__typename` would have told it: T×B×2. At the measured 0.58s per round trip that is ~17s of wall clock before a single issue is read, on both `lazyspec fetch` and the TUI background poll, which share `sync_all` (RFC-057).

This is RFC-065's thinnest end-to-end path: config in, one composed document out, cache written, request count observably down. It stands up `src/engine/gh_fetch.rs`, the query builder and the partial-response error model against the cheapest subtree — flat selections only, so the 500,000-node budget is not reachable and its arithmetic is not yet needed.

The two amplifiers do not get fixed; they cease to exist. The owner subtree is per-repo rather than per-type, so `refresh_schema_snapshot`'s re-fetch collapses by construction. And `owner { __typename ... on Organization ... on User }`, with the same `b<n>: projectV2(number:)` alias in both fragments, makes the account kind a selected field rather than a failed request. Field merging permits the duplicate alias because both fragments resolve `ProjectV2`, verified live: `owner.__typename` returned `User` and `b3.fields.nodes` resolved 13 fields in the same request.

Issue types being org-only becomes a property of the fragment rather than a caveat. A user-owned repo simply has no `issueTypes` key — no error, no wasted request — and the "expected failure on a user repo" note at `issue_cache.rs:299-302` retires.

Value: both fetch surfaces get ~17s shorter on a 10-type project, and the composed-request machinery the rest of RFC-065 rides on is proven by a real cut-over instead of landing dark.

## Acceptance Criteria

- **Given** a repo with 10 configured `github-issues` types, one `github-milestones` type and one authority board
  **When** fetch runs
  **Then** exactly one `gh` invocation serves milestones, org issue types and every board's field schema — counted by a `gh` shim on `PATH` — where the same configuration costs 31 today.

- **Given** a user-owned repo
  **When** fetch runs
  **Then** `owner.__typename` resolves `User`, the board's fields resolve through the `User` fragment in that same request, no second org/user probe is made, no `issueTypes` key is present, and no warning is emitted; `try_org_then_user`, `gh_schema::fetch_snapshot` and `gh_schema::fetch_project_fields` no longer exist.

- **Given** an org-owned repo
  **When** fetch runs
  **Then** `issueTypes` resolves once for the whole round and populates `GhSchemaSnapshot` identically to today, regardless of how many types are configured.

- **Given** the `project` scope is missing, or a board number resolves to nothing
  **When** fetch runs
  **Then** GraphQL returns partial `data` with `errors[].path` naming `repository.owner.b<n>`; the milestone and issue-type subtrees still land, a warning names the board and states the prior schema is kept, `gh-schema.json` retains its previous field, option and iteration ids through the existing merge-not-overwrite path (`issue_cache.rs:326-328`), and board-bound docs keep their last known status (`issue_cache.rs:459-467`).

- **Given** `gh api graphql` exits non-zero while still printing `data` on stdout
  **When** the round is parsed
  **Then** the partial payload is used and the round succeeds — `GhCli::graphql` (`gh.rs:1667-1683`) already parses stdout first and returns `Ok(json)` whenever `data` is present, so no seam change is made and none is needed.

- **Given** identical repo state
  **When** fetch runs before and after this change
  **Then** the milestone cache and `gh-schema.json` on disk are equivalent — same milestones and numbers, same field ids, option ids and iteration ids — so the only observable difference is request count and wall clock.

- **Given** the TUI background poll rather than the CLI
  **When** it fires
  **Then** it makes the same single request with the same warnings, because both surfaces call `sync_all` and `fetch_round` runs once inside it before the per-type dispatch. No per-surface wiring is added.

- **Given** any operation in this slice
  **When** its result is serialized
  **Then** it is available via `--json`, and each warning appears in that type's `warnings` array exactly as the per-request paths emit today.

## Scope

### In Scope

- `src/engine/gh_fetch.rs`: `FetchSnapshot`, `fetch_round(gh, repo, types, boards, cursors)`, the config-driven query builder, and the response parser. `milestones`, `issue_types`, `board_fields` and `warnings` are populated; the issue-shaped fields exist and stay empty until the next slice.
- The owner subtree: `__typename` plus `Organization` and `User` inline fragments carrying the same `b<n>: projectV2(number:)` alias, with `issueTypes` only on the org fragment.
- `errors[].path` mapped to per-subtree warnings, reusing the strings the per-request paths emit today.
- `fetch_round` invoked once in `sync_all` before the per-type dispatch; `GhMilestoneSync` and `refresh_schema_snapshot` read the snapshot instead of calling clients.
- Deleting `gh_schema::try_org_then_user`, `gh_schema::fetch_snapshot` and `gh_schema::fetch_project_fields`. `GhSchemaSnapshot` and its resolvers stay, populated from `FetchSnapshot`.
- Fixture-driven tests over recorded responses — full, partial-with-`errors`, org-owned, user-owned — asserting `FetchSnapshot` construction, plus shim-based request-count assertions on both surfaces.
- ADRs for "partial GraphQL responses are the error model" and "owner account kind is a selected field, not a failed request".

### Out of Scope

- Issue lists and enrichment. `discover_issues` and `IssueCache::fetch_all`'s four read call sites keep their current reads; only the milestone and owner subtrees move.
- Nested connections, the node budget and truncation warnings. Nothing selected here is a per-issue connection, so the 500,000-node cap is unreachable.
- Pagination and `FETCH_LIMIT`.
- Mutation read-back. `issue_view` stays the single-doc read after a write.
- Removing `GhMilestoneApi::milestone_list` or `GhIssueReader::issue_list` from their traits. Both are retained, just no longer called by fetch.
- Comment fetching, parallelism, cache-format changes, ClickUp and git-ref syncers.

### Split Point

Cut at the board schemas. Milestones plus `issueTypes` through the owner subtree is already a working composed request that kills the per-type re-fetch and the org/user double call; `b<n>: projectV2` with its partial-failure warning then follows as a second story.

