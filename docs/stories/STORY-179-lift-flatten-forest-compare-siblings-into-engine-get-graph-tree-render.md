---
title: Lift flatten_forest/compare_siblings into engine; GET /graph tree render
type: story
status: accepted
author: jkaloger
date: 2026-06-30
tags: []
related:
- implements: RFC-052
---## Context

RFC-052 renders the relationship graph as a topologically-sorted tree mirroring the TUI's graph. Today that ordering is built in two stages: `resolve_forest`/`topo_order` already live in `engine/context.rs` (reusable), but `flatten_forest`/`compare_siblings` (DFS flatten + sibling sort) are trapped in `tui/state/graph.rs`. To render the same ordering in HTML without `web -> tui`, this story lifts that logic into `engine` so TUI and web consume one ordering implementation -- the second concrete consumer that justifies the move (convention principle 6). It then adds `GET /graph` walking the ordered nodes into a nested tree. Depends on STORY-176.

**The lift is not just two functions.** `flatten_forest` returns `Vec<GraphNode>` (today defined in `tui/state/app.rs`) and both functions take a sort descriptor (`GraphSort`/`SortKey` in `tui/state/graph.rs`). Those types must move into `engine` alongside the functions, or AC1 ("no TUI dependency") is unsatisfiable. The TUI then re-imports the moved types and functions. This story makes that type relocation explicit and in scope.

The move must be behavior-preserving: the TUI re-points at the moved code with no change to its rendered ordering, verified by the existing graph ordering tests relocating with the functions.

## Acceptance Criteria

- **Given** `flatten_forest`/`compare_siblings` and the types they touch (`GraphNode`, `GraphSort`, `SortKey`) currently in `tui/`
  **When** the refactor completes
  **Then** all of them live in `engine::graph` with no dependency on any `tui` type, and `engine::graph` compiles without the `tui` module.

- **Given** the moved functions and types
  **When** the TUI builds its graph view
  **Then** it imports them from `engine::graph` and renders the same node ordering as before the move.

- **Given** the graph ordering tests currently in `tui/state/graph.rs`
  **When** the functions move to `engine`
  **Then** those tests move with them, run against `engine::graph`, and pass unchanged.

- **Given** a document graph containing a diamond (a node reachable by two paths)
  **When** `GET /graph` renders
  **Then** the shared node is rendered without duplicate-subtree recursion (it may appear as a plain row on the second branch, but its subtree is not re-emitted), matching the TUI.

- **Given** a document graph containing a cycle (including a rootless cyclic component)
  **When** `GET /graph` renders
  **Then** the render terminates, every node appears exactly once, and the back-edge is dropped per the existing ordering logic.

- **Given** a running `serve` instance
  **When** a client requests `GET /graph`
  **Then** the response renders the graph as a topologically-sorted nested `<ul>` tree, ordered by `GraphSort::default()` (the static web view uses the default sort, since it has no interactive sort control), matching the TUI's default-sort ordering.

- **Given** the `web` module
  **When** its imports are inspected
  **Then** it reaches the ordering logic through `engine`, never through `tui`.

## Scope

### In Scope

- Move `flatten_forest`, `compare_siblings`, and the types they depend on (`GraphNode`, `GraphSort`, `SortKey`) from `tui/` into `engine::graph`.
- Relocate the existing graph ordering tests alongside the functions.
- Re-point the TUI at the moved code (pure refactor, no behavior change).
- `GET /graph` route rendering the ordered nodes as a nested `<ul>` tree using `GraphSort::default()`.
- `askama` template for the tree view.

### Out of Scope

- Any change to graph ordering semantics (Kahn's algorithm, diamond/cycle handling, sibling comparison rules stay identical).
- `resolve_forest`/`topo_order` -- already in engine, reused unchanged.
- Interactive sort selection on the web `/graph` view (default sort only; interactive sort stays a TUI concern).
- Mermaid/diagram rendering of the graph (tree is plain nested lists).
