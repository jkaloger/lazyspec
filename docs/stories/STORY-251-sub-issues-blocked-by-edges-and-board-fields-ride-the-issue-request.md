---
title: Sub-issues, blocked-by edges and board fields ride the issue request
type: story
status: in-progress
author: Jack Kaloger
date: 2026-08-06
tags: []
related:
- implements: RFC-065
---

## Context

The preceding commit took blocked-by and project-field reads from `O(N)` to `O(N/100)` via `nodes(ids:)`. That is a real improvement and also that approach's ceiling: `nodes(ids:)` needs the ids first, so it is structurally a second round trip after discovery. Three of them — `fetch_subissue_parentage`, `list_blocked_by_batch` and `reconcile_project_fields_into_cache` each cost T×⌈N/100⌉ requests, ~30 on a 10-type project.

GraphQL exposes `subIssues`, `blockedBy` and `projectItems` directly on `Issue`, so the ids never leave the server. Selecting them inline on the aliases the previous slice built takes all three enrichment steps to zero additional requests, and retires the batch scaffolding whose only purpose was making those reads cheaper: `GH_NODES_BATCH_MAX`, `project_items_batch`, `list_blocked_by_batch`, their queries and parsers, `sync::PrefetchedProjectItems`, and `store_dispatch::reconcile_target_node_id`. `reconcile_project_fields_for_meta` stays — it is the injection logic, not the read — fed from the snapshot.

Nesting is where the node budget bites. GitHub caps a query at 500,000 *possible* nodes, computed from `first:` arguments rather than from what returns. Measured: one type at `subIssues(100) blockedBy(100) projectItems(50) fieldValues(50)` passes; two types at that selection is rejected at 550,200. `projectItems(50) × fieldValues(50)` alone is 2,550 of the 2,750 nodes per issue. Capping to `subIssues(50) blockedBy(50) projectItems(10) fieldValues(25)` gives 360 per issue, verified OK at 10 types and rejected at 16. So the budget is computed from the selection constants and the type count, and types split across ⌈T/12⌉ requests when a project exceeds it, rather than a hardcoded assumption that one request always fits.

The caps are lower than today's. An issue with more than 50 sub-issues or on more than 10 boards would be truncated where today it would not. No observed project reaches that, but silent truncation would corrupt a cache, so each capped connection selects `pageInfo { hasNextPage }` and a `true` becomes a warning naming the document and the connection.

Value: enrichment stops costing requests at all. With the two preceding slices this puts a full fetch at one request per page round — the ~51 requests and ~30s of today against 1.16s verified live for 10 types plus milestones, issue types and board fields in a single request.

## Acceptance Criteria

- **Given** 10 configured types with sub-issue parentage, blocked-by edges and authority-board membership
  **When** fetch runs
  **Then** enrichment adds zero requests to the round, a `gh` shim records no `nodes(ids:)` call, and `GH_NODES_BATCH_MAX`, `project_items_batch`, `list_blocked_by_batch`, `sync::PrefetchedProjectItems` and `store_dispatch::reconcile_target_node_id` no longer exist.

- **Given** identical repo state
  **When** fetch runs before and after this change
  **Then** the nested sub-issue layout, the `blocks`/`blocked-by` relations and every `PROJECT-n.<field>` attribute are equivalent to the batch path's output, including board-derived status on a type with a `status_authority` (STORY-248).

- **Given** 16 configured types
  **When** the round is built
  **Then** the budget arithmetic detects the overrun from the selection constants and the type count, splits the types across ⌈T/12⌉ requests, and no request is rejected with `exceeds the maximum limit of 500,000` — asserted at the query-builder level with no network.

- **Given** an issue with more than 50 sub-issues, or on more than 10 boards, or with more than 25 field values on a board item
  **When** fetch runs
  **Then** the connection is truncated at the cap and a warning names the document and the connection, so the loss is reported rather than written silently into the cache.

- **Given** the `project` scope is absent
  **When** fetch runs
  **Then** `projectItems` returns null with `errors[].path` naming it, the project-scope warning is emitted as today, board-bound docs keep their last known status (`issue_cache.rs:459-467`), and sub-issues and blocked-by edges still land from the same response.

- **Given** a doc with a `targets` milestone or a `member-of` board relation
  **When** its fields are injected
  **Then** `reconcile_project_fields_for_meta` runs per doc unchanged, reading from the snapshot rather than from a prefetched batch.

- **Given** a dependency mutation followed by a fetch
  **When** the fetch runs
  **Then** the resulting `blocked-by` edges come from the round's inline `blockedBy` selection, and the mutation path itself issues no dependency read of its own.

- **Given** the TUI background poll rather than the CLI
  **When** it fires
  **Then** it makes the same one request per page round with the same warnings, by construction through `sync_all`.

- **Given** any operation in this slice
  **When** its result is serialized
  **Then** it is available via `--json`, including the connection-truncation warnings.

## Scope

### In Scope

- `subIssues(first: 50) { nodes { id number } }`, `blockedBy(first: 50) { nodes { number } }` and `projectItems(first: 10) { nodes { id project { number } fieldValues(first: 25) { ... } } }` inline on each type's alias, with the caps on a `Connection` enum whose variants also supply each connection's GraphQL field name for warnings.
- `FetchSnapshot::sub_issues`, `blocked_by` and `project_items` populated and consumed by `IssueCache::fetch_all` in place of its three enrichment reads.
- Node-budget arithmetic from the selection constants and type count, splitting types across ⌈T/12⌉ requests when the 500,000-node cap would be exceeded.
- `pageInfo { hasNextPage }` on every capped connection, and a truncation warning naming the document and connection when it is true.
- Deleting `GH_NODES_BATCH_MAX`, `project_items_batch`, `list_blocked_by_batch`, their queries and parsers, `sync::PrefetchedProjectItems` and `store_dispatch::reconcile_target_node_id`. Keeping `reconcile_project_fields_for_meta`, fed from the snapshot.
- Fixture tests per enrichment shape, a truncation fixture, a null-`projectItems` partial-failure fixture, and a pure budget-arithmetic test at 10 and 16 types.
- ADRs for "one composed GraphQL document is the fetch read layer" and "nested connections are capped below GitHub's node budget, and truncation warns".

### Out of Scope

- A per-issue follow-up query for overflowing connections. The escape hatch is recorded in RFC-065's risks and deliberately not built; truncation warns instead.
- `issue_view` read-back on the mutation path. Retained.
- Parallelism, comment fetching, cache-format changes, and the `IssueMap`, lock and nested-layout shapes.
- Raising the caps or making them configurable.

### Split Point

Cut at project items. Sub-issues and blocked-by inline first — 100 of the 360 nodes per issue, cheap enough to need no split arithmetic — then `projectItems` with `fieldValues`, the budget arithmetic and the truncation warnings as a second story.


