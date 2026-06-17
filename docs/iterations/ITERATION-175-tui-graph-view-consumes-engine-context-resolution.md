---
title: TUI graph view consumes engine context resolution
type: iteration
status: accepted
author: agent
date: 2026-06-18
tags:
- tui
- graph
- context
related:
- implements: STORY-123
---

## Goal

Make the TUI graph view build from the engine's `resolve_forest` (ITERATION-174) instead
of its own `traverse_dependency_chain`, so the graph surfaces the same node and edge set
the CLI `context` command does: multi-parent `implements` edges instead of single-edge
collapse, and `related-to` connections as node annotations. Delivers STORY-123.

Depends on ITERATION-174 (engine `src/engine/context.rs` with `resolve_forest`,
`ResolvedContext`, `ContextNode`). Do not start until that module exists.

## Changes

1. **Flatten the engine forest into the renderable node list.**
   - `rebuild_graph` (`src/tui/state/app.rs:618`) currently seeds roots and calls
     `traverse_dependency_chain` (`src/tui/state/graph.rs`) to fill `Vec<GraphNode>`.
     Replace the traversal source with `crate::engine::context::resolve_forest(&self.store)`.
   - The engine returns a DAG (`Vec<ContextNode>` carrying each node's `parents`). The
     graph view renders a flat `Vec<GraphNode>` with a `depth` field
     (`src/tui/state/app.rs:119`) that `draw_graph` uses for indentation and last-sibling
     connectors. Add a flatten step that walks the forest roots-first, depth-first,
     assigning `depth` by tree level, mirroring the CLI `render_tree` traversal in
     `src/cli/context.rs` (roots sorted by path, children sorted by path).
   - Multi-parent handling: a node reachable by more than one parent is drawn in full on
     first encounter and as a one-line reference on subsequent encounters, matching the
     CLI's `↳ <ID> (see above)` behaviour. Add a `GraphNode` field to mark a node as a
     back-reference (e.g. `reference: bool`) so `draw_graph` renders it without recursing;
     the existing `path`/`title`/`status`/`depth` fields are reused.
   - Remove `traverse_dependency_chain` from `src/tui/state/graph.rs` and its re-export in
     `src/tui/state.rs` once nothing references it.
   - ACs: multi-parent edges shown; backward compatible for single-parent chains;
     deterministic ordering; cycle safety (inherited from `resolve_forest`).

2. **Surface `related-to` as node annotations.**
   - `resolve_forest` is `implements`-only; related-to membership for a node comes from
     the store's `related-to` links. For each rendered node, look up its `related-to`
     neighbours (`store.forward_links` / `store.reverse_links` filtered to `RelatedTo`,
     or a depth-1 `resolve_chain` related set) and render them as an inline annotation on
     the node line, per RFC-006 Graph mode Phase 1 ("cross-cutting relations as
     annotations, not drawn edges"). Use the legend glyph already specified in RFC-006
     (`┄▷ related-to`).
   - Annotations are display-only; they are not separate selectable `graph_nodes` (keeps
     `graph_selected` / `j`/`k` navigation indices stable). If a related target is itself
     a node in the forest, it is not duplicated as an annotation target — annotate only
     cross-cutting links not already on the `implements` tree.
   - AC: documents connected only by `related-to` are surfaced consistent with the
     `context` command's related set.

3. **Update the graph renderer for references and annotations.**
   - `draw_graph` (`src/tui/views/panels.rs:1543`) renders each `GraphNode`. Extend it to:
     (a) render a back-reference node as a single dimmed `↳ <ID> (see above)` line with no
     type icon, and (b) append the `related-to` annotation span(s) after the
     title/status. The `is_last`/connector logic keyed on `depth` is unchanged.
   - AC: rendering is backward compatible when there are no multi-parent or related-to
     links.

## Test Plan

- **Forest flatten unit tests, AC: tasks 1, 3.**
  Test the flatten function (forest `Vec<ContextNode>` → `Vec<GraphNode>`) directly,
  isolated from ratatui. Cover: single chain (depth increments, no references), diamond
  (shared node full once + one back-reference), multi-root ordering (path-sorted),
  multi-parent edge retention, cycle termination. Deterministic and specific (assert the
  exact `(path, depth, reference)` sequence).

- **`rebuild_graph` integration test, AC: tasks 1, 2.**
  Build an `App` over an in-memory store with a multi-parent doc and a `related-to` link;
  assert `app.graph_nodes` contains the multi-parent node once as a full node plus a
  back-reference, and that the `related-to` neighbour is reflected (annotation data
  present on the node). Compare the node set against `context <root>`'s forest to assert
  parity for the `implements` + related dimensions.

- **Render snapshot, AC: task 3.**
  A focused assertion on `draw_graph` output (via the existing TUI test harness used by
  `panels.rs` tests) for a small fixture: back-reference line format and annotation
  suffix. Behavioural, not a full-screen golden, to stay robust to unrelated layout.

- **Regression, AC: backward compatibility.**
  Existing graph-view tests (the `traverse_dependency_chain` / graph tests, e.g. around
  `src/tui/state/graph.rs` and `panels.rs` graph tests) must pass with single-parent
  fixtures producing identical flat output.

## Notes

- The DAG-as-tree problem is already solved once in `src/cli/context.rs#render_tree`
  (roots-first DFS, diamond `(see above)` references). The flatten step should reuse that
  shape so the TUI and CLI agree on which node is drawn in full and which is a reference.
- Keeping annotations non-selectable preserves the `graph_selected` index contract and
  the `Enter`-to-jump behaviour in `navigate_to_relation`. Revisit only if RFC-006
  Phase 2 (canvas edge drawing) is taken up — out of scope here.
- `resolve_forest` returning a flat topological order is not sufficient for render depth;
  the flatten step must compute tree depth from the parent edges, not from topo position.
