---
title: TUI Graph View
type: spec
status: draft
author: jkaloger
date: 2026-03-25
tags:
- tui
- graph
- navigation
related:
- related-to: STORY-015
---



## Summary

The Graph view is a TUI mode that renders the project's document hierarchy as a navigable dependency tree. It is reached by cycling through view modes with the backtick key. On entry, the graph rebuilds from scratch by scanning the store for root documents and walking `implements` edges depth-first.

## Root identification

A root document is any document whose own `related` frontmatter contains no `implements` entry (@ref src/tui/state/app.rs#rebuild_graph). The set of roots is sorted first by document type (lexicographic on the type name string), then by title within each type (@ref src/tui/state/app.rs#rebuild_graph). This ordering determines the top-level sequence of trees in the flattened node list.

## Tree construction

From each root, `traverse_dependency_chain` performs a depth-first walk following reverse `implements` links (@ref src/tui/state/graph.rs#traverse_dependency_chain). At each node, the function queries `Store::referenced_by` for documents that reference the current node via an `Implements` relation (@ref src/engine/store/links.rs#referenced_by). Children at each level are sorted alphabetically by title. A `HashSet<PathBuf>` tracks visited paths so that cycles and shared parents produce only one node in the output, preventing infinite recursion.

Each visited document is pushed onto a flat `Vec<GraphNode>` (@ref src/tui/state/app.rs#GraphNode). A `GraphNode` carries the document path, title, doc type, status, and a `depth` field recording how many `implements` hops separate it from its root.

## Layout

`draw_graph` splits the terminal area into two horizontal panels: a 20% left panel titled "Graph" (currently empty) and an 80% right panel titled "Dependency Graph" (@ref src/tui/views/panels.rs#draw_graph).

Each node renders as a single `ListItem`. For nodes at depth > 0, the renderer prepends indentation: three spaces per ancestor level above the first, then a box-drawing connector. The connector is `└─▶` for the last sibling at a given depth, `├─▶` for non-last siblings. "Last sibling" is determined by checking whether the next node in the flat list has a depth less than or equal to the current node's depth.

After the connector, each node displays a type icon (looked up from `type_icons`, falling back to `○`), the document title, and the status string coloured by `status_color` (@ref src/tui/views/panels.rs#draw_graph).

The selected node is highlighted with cyan foreground and bold modifier. Selection state is managed by `graph_selected`, an index into the flat `graph_nodes` vector (@ref src/tui/state/app.rs#graph_selected).

## Navigation

The graph view handles keys in `handle_graph_key` (@ref src/tui/views/keys.rs#handle_graph_key). `j`/Down and `k`/Up move selection through the flat node list, clamped to bounds. `g` jumps to the first node, `G` to the last.

Pressing Enter on a selected node switches to Types mode: the app looks up the node's document type, selects that type tab, rebuilds the doc tree, finds the matching document by path, and sets it as the selected document (@ref src/tui/views/keys.rs#handle_graph_key). This makes Enter a "jump to document" action.

Pressing `e` opens the selected node's document in the external editor. `q` quits, and backtick cycles to the next view mode (@ref src/tui/state/app.rs#cycle_mode).

## Rebuild trigger

The graph rebuilds each time the user cycles into Graph mode via `cycle_mode`. There is no incremental update; the entire node list is reconstructed from the store (@ref src/tui/state/app.rs#rebuild_graph).
