---
title: "TUI State and Navigation"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: [tui, state, navigation]
related:
  - related-to: docs/stories/STORY-009-flat-navigation-model.md
  - related-to: docs/stories/STORY-012-mode-system-and-view-switching.md
  - related-to: docs/stories/STORY-042-tui-expandable-tree-nodes.md
  - related-to: docs/stories/STORY-048-scroll-padding-and-half-page-navigation.md
  - related-to: docs/stories/STORY-015-graph-mode.md
---

## Summary

The TUI holds all runtime state in a single `App` struct. Navigation flows through a priority-based key dispatch chain, a cyclic `ViewMode` enum, and a flat document tree built from the store. This spec describes the structural invariants governing state, dispatch, and tree construction.

## App: The God-Struct

@ref src/tui/state/app.rs#App

`App` owns every piece of mutable state the TUI needs: the document store, selection indices, overlay/modal structs, caches, and view mode. It is constructed once at startup with `App::new`, which initializes the search index and builds the initial document tree.

Key navigation fields:

- `selected_type` and `selected_doc` track the current position in the type list and document tree respectively.
- `doc_list_offset` and `doc_list_height` manage virtual scrolling for the document list panel.
- `scroll_offset` controls the vertical scroll position in both the preview pane and fullscreen view.
- `expanded_parents: HashSet<PathBuf>` tracks which parent documents have been expanded in the tree.

@ref src/tui/state/app.rs#SCROLL_PADDING

Viewport adjustment uses a constant `SCROLL_PADDING` of 2 lines. When the selected item approaches the top or bottom edge of the visible area, the viewport shifts to maintain at least 2 lines of context between the cursor and the viewport boundary.

## ViewMode and Mode Cycling

@ref src/tui/state/app.rs#ViewMode

The `ViewMode` enum defines the available screen layouts: `Types`, `Filters`, `Metrics`, `Graph`, and (when the `agent` feature is enabled) `Agents`. The backtick key cycles through modes in that order via `cycle_mode`, which calls `ViewMode::next()`. The `Graph` and `Filters` views are covered in SPEC-020 and SPEC-021 respectively.

Mode transitions carry side effects. When entering `Graph` mode, `rebuild_graph` is called to construct the dependency tree. When entering `Filters` mode, the available tag set is computed from the store and `selected_doc` resets to 0. When leaving `Filters` mode, active filters are cleared.

## Key Dispatch Priority Chain

@ref src/tui/views/keys.rs#handle_key

`handle_key` is the single entry point for all keyboard input. It implements a strict priority chain using early returns: each modal state is checked in order, and the first active modal consumes the event. The priority order is:

1. Help overlay (any key dismisses it)
2. Warnings panel
3. Create form
4. Delete confirmation
5. Status picker
6. Link editor
7. Agent dialog (feature-gated)
8. Search mode
9. Fullscreen document view
10. Normal mode (dispatched further by `ViewMode`)

Within normal mode, `handle_normal_key` dispatches to mode-specific handlers for `Filters`, `Graph`, and `Agents`. The `Types` and `Metrics` modes fall through to a shared match block that handles type switching (`h`/`l`), document navigation (`j`/`k`), tree expansion (space), and overlay triggers.

## Document Tree Construction

@ref src/tui/state/app.rs#DocListNode

@ref src/tui/state/app.rs#build_doc_tree

The document list is a flat `Vec<DocListNode>` built by `build_doc_tree`. Each node carries a `depth` (0 for top-level, 1 for children), an `is_parent` flag, and an `is_virtual` flag for synthesized folder parents.

Construction filters the store for the currently selected `DocType`, sorts by date, then iterates top-level documents. Documents that have a parent (determined by `store.parent_of`) are skipped at the top level. For each top-level document, `store.children_of` determines whether it is a parent. If the parent is in the `expanded_parents` set, its children are appended at depth 1.

The space key toggles expansion. On a collapsed parent, it inserts children. On an expanded parent, it collapses and calls `clamp_selected_doc` to ensure the selection index remains within bounds. On a child node, space navigates up to the parent and collapses it.

## Flat Navigation Model

In Types mode, navigation follows a flat two-axis model. `h`/`l` (or arrow keys) switch the selected document type, which rebuilds the document tree and resets `selected_doc` to 0. `j`/`k` move through the flattened document tree. There is no panel focus concept; both axes are always active.

## Scroll Management

@ref src/tui/state/app.rs#adjust_viewport

`adjust_viewport` keeps the selected document visible within the scrollable list panel. It enforces `SCROLL_PADDING` lines of context at both edges. `half_page_down` and `half_page_up` jump by `doc_list_height / 2` items and then call `adjust_viewport` to reconcile the offset.

Fullscreen mode uses a separate `scroll_offset` (u16) with `j`/`k` for line-by-line scrolling, `g`/`G` for top/bottom, and `Ctrl-d`/`Ctrl-u` for half-page jumps based on `fullscreen_height`.

