---
title: 'Bidirectional anchored forest: reverse chain as inverted subtree across engine, TUI, web, CLI'
type: iteration
status: complete
author: agent
date: 2026-07-29
tags: []
related:
- implements: STORY-247
---

## Objective

Anchored forests emit each anchor's chain ancestors as an inverted subtree under the anchor, marked as reverse edges, and TUI / web / `context --json` render the marker.

## Satisfies

STORY-247 AC1-9 (all). Single slice: the engine change is ~one function plus a marker field; the three renderers each consume that field in a few lines because STORY-179 already made them share `flatten_forest`.

## Context

- Parent story (AC text lives there, do not restate): STORY-247. Parent RFC: RFC-049 ("Design" → anchor extent), whose descendants-only extent this widens.
- Convention: `lazyspec convention` + dictum 3 (engine holds traversal; CLI and TUI never depend on each other) and dictum 6 (no indirection for one use).
- Engine:
  - `src/engine/context.rs:7 ContextNode { doc, parents }` — gains the reverse-edge set.
  - `src/engine/context.rs:301 resolve_forest_anchored` — currently BFS down `children` from each anchor and prunes parent edges outside the kept subtree. The upward pass goes here.
  - `src/engine/context.rs:270 chain_parents` — the adjacency the upward walk reuses. No new store reads.
  - `src/engine/graph.rs:21 GraphNode`, `:196 flatten_forest`, `:372 walk` — marker propagation; `drawn`/`on_stack` already handle re-encounters and cycles (AC3, AC7).
- Consumers:
  - TUI: `src/tui/views/panels.rs:1838 graph_doc_cell_spans` (tree cell art), `:1891 draw_graph`. State: `src/tui/state/app.rs rebuild_graph` (already anchors; no change expected).
  - Web: `src/web/render.rs:293 GraphTreeNode` + `:320 nest`, `templates/graph_node.html`, `static/lazyspec.css` (`.graph-tree` styling from ITERATION-244).
  - CLI: `src/cli/context.rs:63 run_forest_json` (`implements_in_context` edges), `:304 run_forest_human`.
- README: `--anchor` rows at `README.md:284,341,343` state "chain descendants ... pruning ancestors above it" — now wrong.
- Touch: `src/engine/context.rs`, `src/engine/graph.rs`, `src/tui/views/panels.rs`, `src/web/render.rs`, `templates/graph_node.html`, `static/lazyspec.css`, `src/cli/context.rs`, `README.md`, `tests/integration/web_serve_test.rs`.

## Tasks

1. `ContextNode`: add `reverse_parents: Vec<PathBuf>` — the subset of `parents` whose edge was inverted by anchoring. Every existing construction site sets it empty; `resolve_forest`/`resolve_forest_by_tag` unanchored output is unchanged (AC6).
2. `resolve_forest_anchored`: after the existing downward BFS, walk `chain_parents` UP from each anchor doc, keeping ancestors not already in the kept set as nodes whose sole parent is the anchor-side node they were reached from, recorded in `reverse_parents`. Guard with a seen set (cycles, diamonds); a fork keeps both upward branches (AC4).
3. `GraphNode`: add `reverse: bool`. In `walk`, compute it at the edge (`child.reverse_parents.contains(parent_path)`), roots false. Diamond re-emission carries the marker of the edge it was reached by.
4. Engine tests in `graph.rs`/`context.rs`: iteration-anchored 3-deep inverted chain with depths (AC1); marker set only on reverse rows (AC2); mid-chain `story` anchor emitting both directions with no duplicate under one parent (AC3); forked upward branches (AC4); tag anchor parity (AC5); `All` forest snapshot unchanged (AC6); cycle terminates (AC7).
5. TUI: `graph_doc_cell_spans` prefixes reverse rows with an upward marker (`▲`) in a distinct style from forward connectors; unit test on the existing `graph_node_fixture`/`graph_node_spans` fixture (AC8).
6. Web: `GraphTreeNode` carries `reverse`; `graph_node.html` emits it (attribute or marker span); CSS styles it consistently with the TUI's read; `web_serve_test.rs` asserts `/graph?pivot=type:iteration` marks reverse rows and `/graph` does not (AC9).
7. CLI: `run_forest_json` emits `reverse_in_context` beside `implements_in_context`; `run_forest_human` shows the marker in the indented tree. Update the three README `--anchor` descriptions.

## Out of scope

- Reverse chain for the `All` anchor, and any toggle keybind (STORY-247 Out of Scope — pivot is the opt-in).
- Re-rooting on `related-to`; `related` stays a column (RFC-049 non-goal).
- `resolve_chain` / `context <id>` behaviour.
- Sort semantics: reverse rows sort as siblings under their anchor via the existing comparator, unchanged.

## Verification

- `cargo run -- context --json --anchor iteration` — each iteration is a root; its story and RFC appear with `reverse_in_context` set.
- `cargo run -- context --json` — forest byte-identical to pre-change output (capture before starting).
- TUI graph pivoted to `iterations`: rows read `ITERATION-x` → `▲ STORY-y` → `▲ RFC-z`; pivoted to `All`: unchanged.
- `/graph?pivot=type:iteration` marks the same rows as the TUI.

