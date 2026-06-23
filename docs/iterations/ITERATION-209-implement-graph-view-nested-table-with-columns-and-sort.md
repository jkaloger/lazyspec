---
title: Implement graph view nested table with columns and sort
type: iteration
status: draft
author: jkaloger
date: 2026-06-23
tags: []
related:
- implements: STORY-154
---<!-- intent: plan the concrete changes that satisfy a story's acceptance criteria -->

## Changes

- Depends ITERATION-205 (attrs), ITERATION-208 (pivot).
- `src/engine/config.rs` tui config: add `tui.graph.columns: Vec<String>` (default `["status","related"]`), `tui.graph.sort: String` (default `"path"`). Serde defaults.
- `src/tui/views/panels.rs` `draw_graph` (`:1555`): render nested table.
  - DOC col = existing tree indent/connectors (`graph_node_spans` `:1501`).
  - Each config column -> cell. Col ids: `status`, `related`, attr names. Unknown-for-row attr -> empty cell.
  - Use ratatui `Table`/aligned columns.
- Sort: `src/tui/state/graph.rs` `flatten_forest` (`:43`) sibling sort (`:55`):
  - comparator keyed by active sort col, tiebreak `path`. Missing attr -> sort last.
  - `App`: `graph_sort_col`, `graph_sort_rev` state.
- `src/tui/views/keys.rs` `handle_graph_key` (`:662`): `o` cycle sort col over `path|status|<attrs>`, `O` reverse. (`s` reserved = status picker.)
- `src/tui/views/keybinds.rs`: register `o`/`O`. Header shows active col + dir arrow.
- README: `tui.graph.columns`, `tui.graph.sort`, keys.

## Test Plan

- AC1: columns config -> table renders DOC + declared cols aligned. render test.
- AC2: col = attr undeclared on a row's type -> empty cell, no panic. render test.
- AC3: `o` cycles sort, `O` reverses; siblings reorder by col, tiebreak path; header shows col+dir. key+state test.
- AC4: siblings w/ missing sort attr -> sort last, deterministic. unit on comparator.

## Notes

- Sort = presentation (TUI), sibling-scoped; engine emits stable topo order.
- Config-declared cols (not auto-all) bounds table width.
- `o`/`O` chosen — `s` taken. Confirm w/ user if other key preferred.
