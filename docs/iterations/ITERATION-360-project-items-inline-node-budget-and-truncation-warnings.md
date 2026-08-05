---
title: Project items inline, node budget and truncation warnings
type: iteration
status: accepted
author: Jack Kaloger
date: 2026-08-06
tags: []
related:
- implements: STORY-251
---

## Objective

`projectItems` with `fieldValues` inline on each alias -> board memberships and field cells cost zero additional requests. Node budget computed from the selection constants and type count, splitting types across `ceil(T/12)` requests when 500,000 would be exceeded. Every capped connection warns on truncation. Retires the last of the `nodes(ids:)` batch scaffolding.

## Context

- Story: STORY-251. Design: RFC-065 §"Node budget" (measured constants: `subIssues(50) blockedBy(50) projectItems(10) fieldValues(25)` = 360 nodes/issue, OK at 10 types, rejected at 16) and §"Per-piece best-effort via `errors[].path`".
- Builds on: STORY-250's aliases, prior iteration's inline `subIssues`/`blockedBy`.
- Touch: `src/engine/gh_fetch.rs` (`projectItems` selection, caps, budget arithmetic, type chunking), `src/engine/sync.rs` (`reconcile_project_fields_into_cache`:476, `PrefetchedProjectItems`:588 delete), `src/engine/store_dispatch.rs` (`reconcile_target_node_id`:634 delete, `reconcile_project_fields_for_meta`:683 refeed), `src/engine/gh.rs` (`GhGraphql::project_items_batch` default :700 + `GhCli` override :1701, `parse_project_items_batch`:1032, `GH_NODES_BATCH_MAX`:992 -- all delete; `parse_project_items_array`:1084 reuse), `src/engine/issue_cache.rs` (`board_owned_status`:459-467 unchanged).

## Satisfies

STORY-251 AC1 (completes the deletions), AC2 (completes -- `PROJECT-n.<field>` attrs and board-derived status), AC3, AC4 (completes -- `projectItems`/`fieldValues` caps), AC5, AC6.

## Tasks

1. Builder: `projectItems(first: PROJECT_ITEMS_CAP) { pageInfo { hasNextPage } nodes { id project { number } fieldValues(first: FIELD_VALUES_CAP) { pageInfo { hasNextPage } nodes { ... } } } }` on each `t<i>` alias. `PROJECT_ITEMS_CAP = 10`, `FIELD_VALUES_CAP = 25` as named consts beside the parsing structs. The `fieldValues` inline-fragment selection is the shape `parse_project_items_array` (gh.rs:1084) already parses -- reuse it, do not write a second parser.
2. `FetchSnapshot::project_items: HashMap<String, Vec<ProjectItem>>` keyed by issue node id.
3. Node budget: a pure fn computing possible nodes from the selection constants and the type count. Over 500,000 -> chunk types across `ceil(T/12)` requests, merging each chunk's snapshot into one. Composes with the cursor rounds from STORY-250: chunking is over types, pagination over rounds; both merge into a single `FetchSnapshot`.
4. `sync::reconcile_project_fields_into_cache` reads `snapshot.project_items`. Delete `PrefetchedProjectItems` -- the wrapper existed only so `reconcile_project_fields_for_meta` could run unmodified against a batch.
5. `reconcile_project_fields_for_meta` keeps its per-doc shape but takes the doc's `&[ProjectItem]` as a param instead of calling `client.project_items`. It still needs `client` for the authority-board-add mutation it can trigger, so the param is added, not swapped. Its target predicate is inlined -- `reconcile_target_node_id` (store_dispatch.rs:634) is deleted, its only two callers being this fn and the batch that is going away.
6. Delete `GhGraphql::project_items_batch` (default + `GhCli` override), `parse_project_items_batch`, `gh::GH_NODES_BATCH_MAX`. `GhGraphql::project_items` then has no production caller left -- STORY-251 does not list it for deletion, so leave it and say so in the PR rather than widening scope unasked.
7. `hasNextPage == true` on `projectItems` or `fieldValues` -> `RefreshWarning` naming the document and the connection.
8. Partial failure: `errors[].path` ending in `projectItems`, or a null value -> the existing project-scope warning; board-bound docs keep their last known status via `board_owned_status` (issue_cache.rs:459-467), and sub-issues plus blocked-by from the same response still land.
9. Tests:
   - Budget arithmetic, pure, no network: 10 types fits one request; 16 types splits into `ceil(16/12) = 2`; asserted at the query-builder level, and the built query's declared node count never exceeds 500,000.
   - Fixtures: board membership + field cells; null-`projectItems` partial failure; truncation (11 boards, 26 field values) -> warnings naming doc + connection.
   - Parity: every `PROJECT-n.<field>` attribute and the board-derived status on a type with a `status_authority` (STORY-248) equivalent to the batch path's output on identical state.
   - `reconcile_project_fields_for_meta` unit tests (store_dispatch.rs:8206) rewritten to pass items directly rather than through a `GhGraphql` double -- the read is no longer its concern, so the seam moves out of the test.
   - `gh` shim on both surfaces: whole fetch is one request per page round.
10. ADRs via `lazyspec create adr`, each linked `implements` RFC-065: "One composed GraphQL document is the fetch read layer" (decision 1) and "Nested connections are capped below GitHub's node budget, and truncation warns" (decision 2).

## Out of scope

- A per-issue follow-up query for overflowing connections. RFC-065 risks record the escape hatch; deliberately not built.
- Raising the caps or making them configurable.
- `GhIssueDependencyApi::list_blocked_by` and `issue_view` on the mutation path. Both retained.
- Parallelism, comment fetching, cache-format changes, `IssueMap`/lock/nested-layout shapes.

## Principles/conventions

CONVENTION.md 2 (truncation warnings serialized through `--json`), 3 (engine; both surfaces inherit via `sync_all`), 4 (fakes only at `GhGraphql`; `reconcile_project_fields_for_meta` stops needing one at all), 6 (batch indirection deleted now that its uses are gone).

Budget arithmetic is a named pure fn, not an inline expression with a comment explaining it. Early returns over nesting in the `fieldValues` variant parsing.

## Verification

- 10 types with parentage, dependencies and board membership: enrichment adds zero requests; shim records no `nodes(ids:)`; full fetch is one request per page round against ~51 today.
- `grep -rn "GH_NODES_BATCH_MAX\|project_items_batch\|PrefetchedProjectItems\|reconcile_target_node_id" src tests` returns nothing.
- 16 types: query builder splits into 2 requests, neither rejected with `exceeds the maximum limit of 500,000`; asserted without network.
- Issue on 11 boards, or a board item with 26 field values: capped, one warning per truncated connection naming the doc.
- Missing `project` scope: `projectItems` null, project-scope warning as today, board-bound docs keep last known status, sub-issues and blocked-by still land from the same response.
- `PROJECT-n.<field>` attributes and `status_authority`-derived status equivalent to the batch path on identical state.
- TUI poll: same request count, same warnings, no per-surface wiring.

