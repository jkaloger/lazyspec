---
title: Graph Mode Tree Rendering and Navigation
type: iteration
status: accepted
author: agent
date: 2026-03-06
tags: []
related:
- implements: docs/stories/STORY-015-graph-mode.md
---



## Scope

Covers STORY-015 AC1 (tree rendering), AC2 (node display), AC3 (j/k navigation), and AC5 (jump to document). Defers AC4 (collapse/expand), AC6 (cross-cutting annotations), AC7 (legend), and AC8 (filter) to a follow-up iteration.

## Changes

### Task 1: Add graph state to App and build the tree

**ACs addressed:** AC1

**Files:**
- Modify: `src/tui/app.rs`

**What to implement:**

Add a `GraphNode` struct to `app.rs`:

```rust
#[derive(Debug, Clone)]
pub struct GraphNode {
    pub path: PathBuf,
    pub title: String,
    pub doc_type: DocType,
    pub status: Status,
    pub depth: usize,
}
```

Add graph state fields to `App`:

```rust
pub graph_nodes: Vec<GraphNode>,
pub graph_selected: usize,
```

Initialise both in `App::new()` (`graph_nodes: Vec::new()`, `graph_selected: 0`).

Add a method `App::rebuild_graph(&mut self)` that:

1. Finds root documents: iterate `self.store.all_docs()`, keep those where `self.store.referenced_by(&doc.path)` has no entry with `RelationType::Implements`.
2. Sort roots by `doc_type` then `title` for stable ordering.
3. For each root, recursively walk children: for each doc, look at `self.store.related_to(&doc.path)` for reverse `Implements` links (i.e. docs that implement this one). Those are children.
4. Flatten the tree depth-first into `self.graph_nodes`, setting `depth` on each node.

Call `rebuild_graph()` inside `cycle_mode()` when entering `ViewMode::Graph`.

**How to verify:**
`cargo test` -- covered by Task 4 tests.

---

### Task 2: Render the dependency tree with box-drawing characters

**ACs addressed:** AC1, AC2

**Files:**
- Modify: `src/tui/ui.rs`

**What to implement:**

Replace `draw_graph_skeleton` with `draw_graph(f: &mut Frame, app: &App, area: Rect)`. Update the call site in `draw()` at line 115.

Layout: 20% left panel / 80% right panel (same as skeleton).

**Left panel:** Render a simple placeholder block titled `" Graph "` (legend comes in iteration 2).

**Right panel:** Render graph nodes as a `List` widget, similar to `draw_doc_list`.

For each `GraphNode` in `app.graph_nodes`, build a `ListItem` containing a `Line` of `Span`s:

1. **Indent + tree characters:** Based on `node.depth`, generate leading whitespace and box-drawing prefixes. For each depth level, use `"   "` (3 spaces) indent. Prefix the node with `" ├─▶ "` or `" └─▶ "` for last-child, `" │  "` for continuation. To determine last-child: look ahead in `graph_nodes` to see if the next node at the same or lower depth follows.
2. **Type indicator:** `●` RFC, `■` ADR, `▲` Story, `◆` Iteration. Style with dim white.
3. **Title:** `node.title` in default style.
4. **Status:** `node.status` display string, coloured using the existing `status_color()` helper.

Use `ListState` with `app.graph_selected` for highlight. Highlight style: `Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)`.

The block title should be `" Dependency Graph "` with the border styled cyan when active.

**How to verify:**
`cargo test` and manual `cargo run` to visually confirm tree rendering.

---

### Task 3: Graph-mode key handling (j/k navigation and Enter to jump)

**ACs addressed:** AC3, AC5

**Files:**
- Modify: `src/tui/app.rs`

**What to implement:**

In `handle_normal_key()`, add a guard for graph mode before the existing key handlers. When `self.view_mode == ViewMode::Graph`:

- `j` / `Down`: increment `graph_selected`, clamped to `graph_nodes.len() - 1`
- `k` / `Up`: decrement `graph_selected`, clamped to 0
- `Enter`: jump to the selected node's document in Types mode. Use the same pattern as `navigate_to_relation()` (lines 355-378):
  1. Get `graph_nodes[graph_selected].path`
  2. Look up the doc in `self.store.get(&path)`
  3. Find the type index in `self.doc_types`
  4. Set `self.selected_type` and find the doc index in `docs_for_current_type()`
  5. Set `self.selected_doc` and switch `self.view_mode = ViewMode::Types`
- `g`: jump to first node (`graph_selected = 0`)
- `G`: jump to last node
- `q`: quit
- Backtick: cycle mode (already handled, but ensure it still works)

All other keys in graph mode are no-ops (return early).

**How to verify:**
`cargo test` -- covered by Task 4 tests.

---

### Task 4: Tests

**ACs addressed:** AC1, AC2, AC3, AC5

**Files:**
- Create: `tests/tui_graph_test.rs`

**What to implement:**

Use the existing `TestFixture` from `tests/common/mod.rs`. Create a fixture with a document hierarchy:

```
RFC-001 -> STORY-001 -> ITER-001
        -> STORY-002
RFC-002 (standalone root)
```

Use `fixture.write_rfc()`, `fixture.write_story()`, `fixture.write_iteration()` with appropriate `implements` links.

**Tests:**

1. `test_rebuild_graph_builds_forest` -- call `app.rebuild_graph()`, assert `graph_nodes` length matches expected count (5 nodes). Assert root nodes have `depth == 0`, children have `depth == 1`, grandchildren `depth == 2`.

2. `test_rebuild_graph_roots_have_no_incoming_implements` -- assert the depth-0 nodes are exactly the two RFCs.

3. `test_graph_navigate_j_k` -- set `graph_selected = 0`, send `j` key, assert `graph_selected == 1`. Send `k`, assert back to 0. Send `k` again, assert still 0 (clamped).

4. `test_graph_navigate_g_G` -- send `G`, assert `graph_selected == graph_nodes.len() - 1`. Send `g`, assert `graph_selected == 0`.

5. `test_graph_enter_jumps_to_types_mode` -- select a story node in the graph, send `Enter`, assert `view_mode == ViewMode::Types`, `selected_type` matches Story index, and `selected_doc` matches the story.

6. `test_graph_rebuilds_on_mode_switch` -- cycle to Graph mode, assert `graph_nodes` is populated. Cycle away and back, assert it's repopulated.

**How to verify:**
`cargo test -- tui_graph`

## Test Plan

| Test | ACs | Properties | Notes |
|------|-----|-----------|-------|
| `test_rebuild_graph_builds_forest` | AC1 | Deterministic, Specific, Behavioral | Verifies tree construction from implements edges |
| `test_rebuild_graph_roots_have_no_incoming_implements` | AC1 | Specific, Predictive | Verifies root detection logic |
| `test_graph_navigate_j_k` | AC3 | Isolated, Fast, Behavioral | Verifies depth-first navigation order |
| `test_graph_navigate_g_G` | AC3 | Isolated, Fast | Verifies jump-to-boundary navigation |
| `test_graph_enter_jumps_to_types_mode` | AC5 | Behavioral, Predictive | Verifies mode switch and document selection |
| `test_graph_rebuilds_on_mode_switch` | AC1 | Structure-insensitive | Verifies graph state lifecycle |

All tests are unit-level, operating on `App` state directly. No rendering tests (ratatui rendering is hard to assert on and the visual output is best verified manually). This trades Predictive coverage of AC2 (node display) for Fast and Writable tests on the data layer. AC2's rendering is verified manually with `cargo run`.

## Notes

The tree-building algorithm walks `referenced_by` in reverse: for a given node, its children are documents whose `related_to` includes an `Implements` link targeting it. The store exposes this through `related_to()` which returns both forward and reverse links. We need to filter for reverse `Implements` links specifically (docs that implement the current doc).

Stable ordering (by type then title) ensures deterministic graph layout across rebuilds.
