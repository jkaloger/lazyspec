---
title: Engine blocks DAG primitives
type: story
status: complete
author: jkaloger
date: 2026-04-30
tags: []
related:
- implements: RFC-041
- blocks: STORY-121
- blocks: STORY-123
- blocks: STORY-114
- blocks: STORY-125
priority: should
---








## Summary

Establish the engine-level graph layer that turns the existing `blocks` and `implements` relationships into a queryable directed acyclic graph over documents. The graph is the substrate every downstream sequencing surface (CLI, TUI, skills) will consume: it answers what is ready to work on now, what is gating delivery, and what becomes unlocked when work completes. This slice delivers the pure, in-memory graph primitives with no I/O and no user-facing surface; it leaves priority configuration, CLI commands, TUI work, and skills to later slices.

## Scope

### In Scope

- A graph type derived from a set of documents, capturing both `blocks` and `implements` edges as distinct relations.
- Cycle detection that reports the offending nodes when a cycle is present.
- Topological ordering over the graph when it is acyclic.
- A smart-traversal "next ready" query that classifies each ready candidate as `claimable`, `needs-children`, or `needs-status-update` by descending `implements`-children rather than filtering by document type.
- A critical-path query that takes externally supplied weights and returns the longest weighted path within a scope.
- Type-aware terminal-status semantics, where what counts as "done" depends on document type (RFC and Story treat `complete`, `superseded`, `rejected` as terminal; Iteration and Audit treat only `complete` as terminal; ADR, Convention, and Dictum treat `accepted` and `superseded` as terminal).
- Lease integration: by default the ready set hides documents currently leased, with an opt-in mode that returns them annotated with the lessee identity.
- A bottleneck diagnostic that surfaces the top three non-terminal documents gating the most downstream candidates.
- Unit tests covering each behaviour above.

### Out of Scope

- The `priority` frontmatter field and `lazyspec.toml` priority configuration (later slice).
- Any CLI subcommands, flags, or human-readable output (later slice).
- The interactive TUI sequencing screen and any rendering concerns (later slice).
- The `/sequence` and `/next-work` skills (later slices).
- Retirement of the existing STORY-015 graph mode (later slice).
- Doc-type configuration plumbing for `requires_priority` (later slice).
- Filesystem reads, document loading, or persistence: this slice operates over an in-memory document set.

## Acceptance Criteria

1. **Given** a set of documents with `blocks` and `implements` relationships, **when** a graph is constructed from them, **then** every document appears as a node and every relationship of either kind is preserved as a typed edge that can be queried independently.

2. **Given** a document set whose `blocks` relationships form no cycle, **when** the cycle check runs, **then** it reports success.

3. **Given** a document set whose `blocks` relationships contain a cycle, **when** the cycle check runs, **then** it reports failure and identifies the documents that participate in the cycle.

4. **Given** an acyclic graph, **when** a topological order is requested, **then** the result lists every node such that no node appears before any of its `blocks`-upstreams.

5. **Given** a graph whose ready candidates have no `implements`-descendants, **when** the next-ready query runs, **then** each candidate is returned with kind `claimable`.

6. **Given** a ready candidate that has no `implements`-descendants but is itself non-terminal while every other implements-sibling chain is exhausted, **when** the next-ready query runs, **then** it is returned with kind `needs-children`.

7. **Given** a ready candidate whose `implements`-descendants are all in their type's terminal status while the candidate itself is not, **when** the next-ready query runs, **then** it is returned with kind `needs-status-update`.

8. **Given** a ready candidate that has incomplete `implements`-descendants, **when** the next-ready query runs, **then** the candidate itself is not returned and its ready descendants are returned in its place, regardless of document type.

9. **Given** a graph and a weighting over nodes, **when** the critical-path query runs, **then** the result is the path through the graph that maximises the cumulative weight, honouring `blocks` ordering.

10. **Given** documents of differing types with statuses drawn from each type's terminal set, **when** terminal-status is evaluated for the purpose of clearing an upstream blocker, **then** an RFC or Story is treated as terminal at `complete`, `superseded`, or `rejected`; an Iteration or Audit only at `complete`; and an ADR, Convention, or Dictum at `accepted` or `superseded`.

11. **Given** a document whose status is `accepted` but whose document type is not a decision artifact, **when** terminal-status is evaluated, **then** it is not treated as terminal.

12. **Given** a ready document that is currently leased, **when** the next-ready query runs with default options, **then** it is excluded from the ready set.

13. **Given** a ready document that is currently leased, **when** the next-ready query runs with the include-leased option enabled, **then** it is returned in the ready set annotated with the lessee identity.

14. **Given** a graph with non-terminal documents that gate downstream work, **when** the next-ready query runs, **then** the result includes a bottleneck list of at most three non-terminal documents, ordered by how many downstream candidates each one gates.

15. **Given** a graph with fewer than three gating non-terminal documents, **when** the next-ready query runs, **then** the bottleneck list contains only those that exist, without padding.
