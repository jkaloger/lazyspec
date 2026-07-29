---
title: Graph pivot surfaces reverse chain lineage as an inverted subtree
type: story
status: complete
author: jkaloger
date: 2026-07-29
tags: []
related:
- implements: RFC-049
---

## Context

RFC-049 fixed the anchored forest's extent as "anchor docs as roots + their decomposition descendants nested" (`resolve_forest_anchored`, `engine/context.rs:301`). That makes downward pivots useful and upward pivots useless: pivoting on `iteration` — the leaf type — renders a flat list of every iteration with no lineage at all, because iterations have no chain descendants. The user pivoting on iterations wants the answer to "what story and RFC does each of these serve?", and today has to leave the graph and run `context <id>` per row.

This story widens the anchored extent to be **bidirectional**: an anchor's chain ancestors are emitted too, as an inverted subtree under the anchor row, so the pivot reads top-down in traversal order (`ITERATION-246 → STORY-184 → RFC-058`) with the anchor still the root. It is an extension of RFC-049's anchor-extent decision, recorded here rather than by amending the accepted RFC.

The whole-store (`All`) forest is unchanged — it already shows full lineage from roots down, so reverse edges there would duplicate every path.

Engine-side this is one change to `resolve_forest_anchored` plus a reverse-edge marker carried on `ContextNode`/`GraphNode`. Because STORY-179 lifted `flatten_forest` into the engine, the TUI graph view, the web `/graph` tree and `context --anchor --json` all consume the same forest — parity is a rendering concern in each, not three traversals.

## Acceptance Criteria

- **Given** a store where `ITERATION-A implements STORY-B implements RFC-C`
  **When** the forest is anchored on type `iteration`
  **Then** `ITERATION-A` is a root, `STORY-B` is its depth-1 child and `RFC-C` its depth-2 child, each marked as a reverse-chain node

- **Given** an anchored forest
  **When** a node is reached by a reverse (ancestor) edge
  **Then** it carries a reverse marker distinguishing it from a forward (descendant) node, and the marker is exposed in `context --anchor <type> --json`

- **Given** a mid-chain anchor type (e.g. `story`) whose docs have both ancestors and descendants
  **When** the forest is anchored on it
  **Then** the anchor's descendants and its ancestors both appear under the anchor, descendants unmarked and ancestors marked, and no node is emitted twice under the same parent

- **Given** an anchor doc whose chain lineage forks (a story implementing two RFCs)
  **When** the forest is anchored on that story's type
  **Then** both upward branches render

- **Given** a tag pivot
  **When** the forest is anchored by tag
  **Then** reverse chain applies identically to the type pivot

- **Given** the `All` anchor
  **When** the graph renders
  **Then** the forest is byte-identical to today's — no reverse edges, no markers

- **Given** a chain cycle in an anchored forest
  **When** it is flattened
  **Then** the walk terminates, every node is emitted at least once, and a node reached by a reverse edge is drawn in full — its row plus the lineage below it — once per rendered parent row (a 3-cycle above four anchors draws each of its nodes under each anchor)

- **Given** the TUI graph view pivoted on a type
  **When** a reverse-chain row renders
  **Then** its tree cell carries an upward edge marker visually distinct from a forward child row

- **Given** the web `/graph` view pivoted on a type
  **When** it renders
  **Then** reverse-chain rows carry the equivalent marker, so TUI and web read the same

## Scope

### In Scope

- `resolve_forest_anchored` (`engine/context.rs`): upward chain walk per anchor, ancestors re-parented under the anchor with the edge inverted
- Reverse-edge marker on `ContextNode` and `GraphNode`, propagated through `flatten_forest`
- TUI graph renderer: upward marker on reverse rows (`tui/views/panels.rs`)
- Web `/graph` renderer: same marker (`web/render.rs`)
- `context --anchor <type> --json`: reverse marker in the emitted forest; README updated for the changed `--anchor` semantics

### Out of Scope

- Re-rooting on `related-to` (RFC-049 non-goal; cross-cutting stays the `related` column)
- Reverse chain in the `All` forest, or a keybind to toggle reverse chain — pivoting is itself the opt-in
- Anchoring on a single document rather than a type/tag
- Changes to `resolve_chain` (the `context <id>` single-doc path already reports the upward chain)

## Non-Functional Notes

- Reverse walk is bounded by the chain-parent adjacency already built by `chain_parents`; no extra store reads, so the pivot rebuild stays O(nodes + edges)
- `related-to` annotations on a reverse-chain node keep their existing lineage-exclusion semantics — an ancestor drawn as an inverted tree edge is on the node's lineage and is not re-surfaced as cross-cutting

