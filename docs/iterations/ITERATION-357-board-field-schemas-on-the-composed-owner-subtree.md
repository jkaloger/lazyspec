---
title: Board field schemas on the composed owner subtree
type: iteration
status: accepted
author: Jack Kaloger
date: 2026-08-06
tags: []
related:
- blocks: ITERATION-358
- implements: STORY-249
---

## Objective

`b<n>: projectV2(number:)` aliased into both owner fragments -> every authority board's field schema rides the same request as milestones and issue types. `try_org_then_user` and `fetch_project_fields` die: account kind becomes a selected field, not a failed request.

## Context

- Story: STORY-249. Design: RFC-065 §"Owner subtree" (the duplicate-alias-in-both-fragments trick, verified live) and §"Per-piece best-effort via `errors[].path`". Prior slice stood up `gh_fetch::fetch_round` and the owner subtree.
- Touch: `src/engine/gh_fetch.rs` (builder + parser), `src/engine/gh_schema.rs` (`try_org_then_user`:219, `fetch_project_fields`:259, `OwnerKind`, `PROJECT_FIELDS_ORG_QUERY`/`PROJECT_FIELDS_USER_QUERY` -- all delete; `parse_project_fields`:259+ reuse), `src/engine/issue_cache.rs` (`refresh_schema_snapshot` board loop, merge-not-overwrite :326-328, `board_owned_status` :459-467), `src/engine/store_dispatch.rs` (`authority_board_numbers`:2414 -- read, unchanged).

## Satisfies

STORY-249 AC1, AC2, AC4. Completes AC6/AC7/AC8 for the board-schema piece.

## Tasks

1. Builder: for each number from `store_dispatch::authority_board_numbers(config)`, emit `b<n>: projectV2(number: <n>) { fields(first: 50) { ... } }` inside BOTH `... on Organization` and `... on User`. Same alias in both -- field merging permits it because both resolve `ProjectV2`. Keep the `fields` selection as one named const shared by both fragments, not two copies.
2. Reuse `gh_schema::parse_project_fields` on each resolved `b<n>` node -> `FetchSnapshot::board_fields: HashMap<u64, (Vec<ProjectFieldId>, Vec<OptionId>, Vec<IterationId>)>`.
3. Parser: null `b<n>`, or `errors[].path == ["repository","owner","b<n>"]`, -> the existing warning string "could not refresh field schema for board {n} (keeping prior, projects need `gh auth refresh -s project`)". Board absent from the snapshot != board with empty fields.
4. `refresh_schema_snapshot`: board loop reads `snapshot.board_fields` and calls `replace_board_fields` per board present. Merge-not-overwrite at :326-328 unchanged -- a board the round did not resolve keeps its prior field, option and iteration ids.
5. Delete `gh_schema::try_org_then_user`, `fetch_project_fields`, `OwnerKind`, `PROJECT_FIELDS_ORG_QUERY`, `PROJECT_FIELDS_USER_QUERY` + their tests. `GhSchemaSnapshot` and its resolvers stay.
6. Tests:
   - Fixtures: org-owned with two boards; user-owned with two boards (assert both resolve through the `User` fragment in one response, `owner.__typename == "User"`, no `issueTypes` key, no warning); partial naming `repository.owner.b7` (assert milestones + issue types still land, one board warning, other board's fields land, prior ids for b7 kept).
   - New integration test infra: a `gh` shim on `PATH` recording invocations. Config of 10 `github-issues` types + 1 `github-milestones` type + 1 authority board -> the milestone/issue-type/board-schema work is exactly 1 invocation (31 today). Assert the same count on the TUI poll path (`tui/infra/event_loop.rs:397`), not just the CLI.
   - Board-bound docs keep last known status when the board schema fails: `board_owned_status` (issue_cache.rs:459-467) unchanged, asserted.

## Out of scope

- Issues, discovery, enrichment -- STORY-250, STORY-251.
- Node budget. Owner subtree is per-repo and flat; nothing here is a per-issue connection.
- Pagination, `FETCH_LIMIT`, mutation read-back, `refresh_stale`.
- Removing `GhMilestoneApi::milestone_list` or `GhIssueReader::issue_list` from their traits.

## Principles/conventions

CONVENTION.md 2, 3, 4, 6 -- same posture as the prior slice: engine-only, `GhGraphql` unchanged, no new trait.

ADR via `lazyspec create adr`, linked `implements` RFC-065: "Owner account kind is a selected field, not a failed request" (RFC-065 decision 5).

## Verification

- User-owned repo (`jkaloger/lazyspec`): one request, `owner.__typename == "User"`, board fields resolve, zero org/user probes, no `issueTypes` key, no warning.
- Org-owned repo: `issueTypes` and board fields in the same one request.
- Missing `project` scope, or a board number resolving to nothing: partial `data`, milestone and issue-type subtrees still land, warning names the board and says prior schema kept, `gh-schema.json` retains that board's prior field/option/iteration ids.
- `gh-schema.json` equivalent before/after on identical repo state -- same field ids, option ids, iteration ids.
- `grep -rn "try_org_then_user\|fetch_project_fields\|OwnerKind" src` returns nothing.
- Shim count on both surfaces: 1, down from 31.

