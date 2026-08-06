---
title: 'Composed gh_fetch round: milestones and org issue types'
type: iteration
status: complete
author: Jack Kaloger
date: 2026-08-06
tags: []
related:
- implements: STORY-249
- blocks: ITERATION-357
---

## Objective

Stand up `src/engine/gh_fetch.rs`: one composed GraphQL doc per fetch round. Milestones + org issue types ride it. `sync_all` runs `fetch_round` once; `GhMilestoneSync` and `refresh_schema_snapshot` read the snapshot instead of calling clients. Per-type issue-types re-fetch collapses by construction.

## Context

- Story: STORY-249. Design: RFC-065 (`FetchSnapshot` shape, `fetch_round` signature, owner-subtree GraphQL, `errors[].path` error model). Architecture built on: RFC-057 (`sync_all`/`TypeSync`/`SyncContext`). Don't re-derive any of it.
- Touch: `src/engine/gh_fetch.rs` (new), `src/engine/sync.rs` (`SyncContext`:42, `sync_all`:279, `GhMilestoneSync`:91-125), `src/engine/milestone_cache.rs` (`fetch_milestones`:16), `src/engine/issue_cache.rs` (`refresh_schema_snapshot`:303, org-only caveat comment :299-302, merge-not-overwrite :326-328), `src/engine/gh_schema.rs` (`fetch_snapshot`:241 delete, `ISSUE_TYPES_ORG_QUERY`, `parse_issue_types`), `src/engine/mod.rs`, `src/cli/fetch.rs`, `src/tui/infra/event_loop.rs:397`.
- Transport seam unchanged: `GhCli::graphql` (gh.rs:1667-1683) already parses stdout first and returns `Ok(json)` when `data` present regardless of exit code. No seam change; don't add one.

## Satisfies

STORY-249 AC3, AC5, AC6 (milestones + issue types), AC7, AC8. AC1, AC2, AC4 need the board-field aliases -> next iteration.

## Tasks

1. New `src/engine/gh_fetch.rs`, registered in `mod.rs`. `FetchSnapshot` with every field from RFC-065's sketch declared; `issues`/`sub_issues`/`blocked_by`/`project_items`/`next_pages` stay empty this slice. `fetch_round(gh: &dyn GhGraphql, repo, types, boards, cursors) -> Result<FetchSnapshot>`. No cache access -- builder + parser only.
2. Query builder: `repository(owner:,name:)` root; `milestones(first: 100, states: [OPEN, CLOSED])`; `owner { __typename ... on Organization { issueTypes(first: 50) { nodes { id name } } } ... on User { } }`. Board aliases next iteration. Field selections as named consts next to the structs that parse them.
3. Parser: each subtree read independently -- absent subtree is not an error. `errors[].path` -> `Vec<RefreshWarning>`, reusing the strings the per-request paths emit today verbatim.
4. `SyncContext` gains `pub fetch: Option<&'a FetchSnapshot>`. `TypeSync`, `SyncOutcome`, the `match StoreBackend` dispatch keep their shapes (RFC-057). `sync_all` calls `fetch_round` once before the backend loop. Round transport failure -> warning on every gh-backed `SyncOutcome`, caches untouched (RFC-065 risk: one failure domain, read-only round).
5. `milestone_cache::fetch_milestones` takes `milestones: &[GhMilestone]` in place of `gh: &impl GhMilestoneApi`. `GhMilestoneSync` drops its `gh` field. `GhMilestoneApi::milestone_list` stays on the trait, just uncalled by fetch.
6. `IssueCache::refresh_schema_snapshot` takes `&FetchSnapshot` in place of `&dyn GhGraphql`; issue types come from `snapshot.issue_types`. Board loop stays on `gh_schema::fetch_project_fields` this slice. Delete the "expected failure on a user repo" caveat at :299-302 -- fragment makes it a non-event.
7. Delete `gh_schema::fetch_snapshot`, `ISSUE_TYPES_ORG_QUERY`, `parse_issue_types` + their tests. `GhSchemaSnapshot` and its resolvers stay, populated from `FetchSnapshot`.
8. Callers: `src/cli/fetch.rs` and `src/tui/infra/event_loop.rs:397` build `SyncContext` the same way -- neither gains fetch logic, both inherit the round from `sync_all` (principle 3: CLI and TUI depend on engine, never on each other).
9. Tests -- parser first, transport second:
   - Fixture-driven unit tests in `gh_fetch.rs` over recorded response JSON: org-owned full, user-owned full, partial with `errors[].path`. Assert `FetchSnapshot` construction directly. No fake transport -- the parser is pure.
   - Trait-seam doubles that now see a composed query rather than narrow methods (RFC-065 risk "test doubles must grow"): `MockGhClient` (gh.rs:2618), `MockReader` (issue_cache.rs:1299), `StubGh` (cli/fetch.rs:998), `NoopGh` (tests/integration/fetch_prune_test.rs:36), `NestingGh` (tests/integration/fetch_nested_subissues_test.rs:63). Each answers with a canned composed doc from its existing `graphql()` impl. Do NOT add a `fetch_round` trait method to widen the seam -- `GhGraphql` is the seam and it is unchanged (principle 4).
   - Milestone doubles (`MockGhMilestoneClient` gh.rs:2168) drop out of the fetch path; keep them for the mutation tests that still use them.
10. ADR via `lazyspec create adr`, linked `implements` RFC-065: "Partial GraphQL responses are the error model" (RFC-065 decision 4).

## Out of scope

- Issue lists, discovery, enrichment -- STORY-250, STORY-251. `discover_issues` and `fetch_all`'s four read call sites keep their current reads.
- `b<n>: projectV2` board schemas, deleting `try_org_then_user`/`fetch_project_fields`, board partial-failure warnings -> next iteration (STORY-249 AC1, AC2, AC4). ADR "owner account kind is a selected field" lands there, where the retry actually dies.
- Node budget arithmetic. Selections here are flat, no per-issue connection, 500,000-node cap unreachable.
- Pagination, `FETCH_LIMIT`, `issue_view` mutation read-back, comment fetching, parallelism, cache-format changes, ClickUp/git-ref syncers.
- `IssueCache::refresh_stale` (called from `main.rs:893`, not from `sync_all`) -- stays on its own reads.

## Principles/conventions

CONVENTION.md principle 2 (`--json` on everything -- warnings land in each type's `warnings` array as today), 3 (engine change; CLI/TUI unchanged and untouched by each other), 4 (`GhGraphql` is the existing trait seam; fakes only there), 6 (no new indirection -- `fetch_round` is a free fn over the existing trait, not a new trait).

Early returns over nesting in the parser branches. No comments restating the code; where a multi-line explanation is tempting for a subtree parse, extract a named fn instead.

## Verification

- Org repo, 10 configured types: `issueTypes` resolves once for the round, not once per type.
- User repo: no `issueTypes` key in the response, no error, no warning, no second probe for issue types.
- `gh-schema.json` issue types and the milestone cache on disk are equivalent before/after -- same milestones, numbers, titles, states, `due_on`/`open_issues`/`closed_issues` attrs.
- `gh api graphql` exits non-zero while printing `data` -> round succeeds, `GhCli::graphql` untouched.
- `lazyspec fetch --json` and the TUI poll produce identical warnings and identical `fetched`/`new`/`removed` counts.
- Round transport failure: previous caches intact, one warning per gh-backed type, exit path unchanged.

