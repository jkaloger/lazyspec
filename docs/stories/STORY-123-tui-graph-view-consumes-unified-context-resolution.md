---
title: TUI graph view consumes unified context resolution
type: story
status: draft
author: jkaloger
date: 2026-06-18
tags:
- tui
- graph
- context
related:
- implements: RFC-006
- related-to: SPEC-020
- related-to: STORY-015
- related-to: STORY-122
- related-to: STORY-124
---

## Context

The TUI graph view renders the project's document hierarchy by walking `implements`
edges with its own traversal (`src/tui/state/graph.rs#traverse_dependency_chain`),
seeded from roots in `rebuild_graph`. That traversal diverges from the resolution the
`lazyspec context` command performs: a shared `visited` set collapses any node reached
by more than one parent to a single edge, so a multi-parent document renders as if it
had one parent, and `related-to` links are never followed, so documents connected only
by `related-to` never appear in the graph at all. The result is that the graph view
omits documents and edges the CLI surfaces.

The shared resolution logic is being lifted out of the CLI into an engine module (a
separate, behaviour-preserving refactor iteration) that exposes both a single-target
neighbourhood walk and a whole-store forest walk over the same primitives. Once that
module exists, the graph view should build its forest from it rather than from its own
traversal, so the graph and the CLI present the same node and edge set.

This story covers only the graph view's adoption of the engine module. The engine
extraction itself, the CLI, and the relations tab are out of scope (see Scope).

## Acceptance Criteria

- **Given** a document that `implements` two parent documents
  **When** the graph view renders
  **Then** both `implements` edges are shown, rather than the document being collapsed
  to a single parent.

- **Given** documents connected only by a `related-to` link (no `implements` path
  between them)
  **When** the graph view renders
  **Then** the `related-to` connection is surfaced on the relevant node as an annotation,
  consistent with the related set the `context` command reports for those documents.

- **Given** the engine context module is available
  **When** the graph view rebuilds its node list
  **Then** it obtains nodes and edges from the engine's whole-store forest resolution,
  and the TUI-local `traverse_dependency_chain` is removed.

- **Given** the same store contents
  **When** the graph view rebuilds twice
  **Then** the node ordering is identical between rebuilds (deterministic ordering is
  preserved).

- **Given** a store containing a cycle of `implements` relations
  **When** the graph view renders
  **Then** traversal terminates and every reachable document appears exactly once.

- **Given** a store with no `related-to` links and only single-parent `implements`
  chains
  **When** the graph view renders
  **Then** the rendered tree is unchanged from the previous `implements`-only behaviour
  (backward compatible for the common case).

## Scope

### In Scope

- The graph view consuming the engine context module's whole-store forest resolution.
- Rendering multiple `implements` edges for multi-parent documents instead of collapsing
  to one.
- Surfacing `related-to` connections as node annotations (RFC-006 Graph mode Phase 1).
- Removing the TUI-local `traverse_dependency_chain`.
- Preserving deterministic ordering and cycle safety.

### Out of Scope

- Extracting the resolution logic into the engine (separate behaviour-preserving refactor
  iteration; this story depends on it).
- Any change to `lazyspec context` CLI output.
- The TUI relations tab (STORY-124).
- Phase 2 canvas-based edge drawing for cross-cutting relations (deferred by RFC-006);
  `related-to` appears as annotations, not drawn edges.
- Rendering `blocks` / `supersedes` edges.
- A depth-N control for `related-to` in the TUI; the graph view uses the default depth.
