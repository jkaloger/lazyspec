---
title: "App State"
type: arch
status: accepted
author: jkaloger
date: 2026-03-15
tags: [architecture, tui, state]
related:
  - related-to: "docs/stories/STORY-012-mode-system-and-view-switching.md"
  - related-to: "docs/stories/STORY-042-tui-expandable-tree-nodes.md"
  - related-to: "docs/stories/STORY-006-create-form-ui-and-input-handling.md"
  - related-to: "docs/stories/STORY-008-delete-confirmation-dialog.md"
  - related-to: "docs/stories/STORY-016-status-picker.md"
  - related-to: "docs/stories/STORY-046-tui-warnings-panel.md"
  - related-to: "docs/stories/STORY-051-agent-invocation-from-tui.md"
---

# App State

`App` is a large state struct holding everything the TUI needs.

@ref src/tui/app.rs#App

## Navigation State
- `selected_type` / `selected_doc` -- current selection in type list and doc list
- `scroll_offset` -- preview scroll position
- `doc_list_offset` / `doc_list_height` -- virtual scrolling for document list
- `search_mode` / `search_query` / `search_results` -- search overlay

## View Modes

See [STORY-012: Mode System and View Switching](../../stories/STORY-012-mode-system-and-view-switching.md).

@ref src/tui/app.rs#ViewMode

```d2
direction: right

types: "Types (default)" {
  desc: "Type selector + doc list + preview"
}
filters: "Filters" {
  desc: "Status/tag filter dropdowns"
}
metrics: "Metrics" {
  desc: "Statistics view"
}
graph: "Graph" {
  desc: "Relationship visualization"
}
agents: "Agents" {
  desc: "Agent workflow (feature-gated)"
  style.stroke-dash: 3
}

types -> filters: "Tab"
filters -> metrics: "Tab"
metrics -> graph: "Tab"
graph -> agents: "Tab (if agent feature)"
agents -> types: "Tab"
```

## Document Tree

The doc list is rendered as a tree with collapsible parents. `doc_tree: Vec<DocListNode>`
is a flat list where each node carries a `depth` and `is_parent` flag. Parents can
be expanded/collapsed, and children inherit the parent's expansion state.
See [STORY-042: TUI expandable tree nodes](../../stories/STORY-042-tui-expandable-tree-nodes.md).

@ref src/tui/app.rs#DocListNode

## Overlays (Modal State Machines)

Several modal dialogs exist as independent state machines:

| Overlay | State Struct | Trigger | Story |
|---|---|---|---|
| Search | `search_mode` + `search_query` | `/` key | |
| Create Form | `CreateForm` | `n` key | [STORY-006](../../stories/STORY-006-create-form-ui-and-input-handling.md) |
| Delete Confirm | `DeleteConfirm` | `d` key | [STORY-008](../../stories/STORY-008-delete-confirmation-dialog.md) |
| Status Picker | `StatusPicker` | `s` key | [STORY-016](../../stories/STORY-016-status-picker.md) |
| Help | `show_help` | `?` key | |
| Warnings | `show_warnings` | `w` key | [STORY-046](../../stories/STORY-046-tui-warnings-panel.md) |
| Fullscreen Doc | `fullscreen_doc` | `Enter` key | |
| Agent Dialog | `AgentDialog` | `a` key (feature-gated) | [STORY-051](../../stories/STORY-051-agent-invocation-from-tui.md) |

@ref src/tui/app.rs#CreateForm

@ref src/tui/app.rs#DeleteConfirm

@ref src/tui/app.rs#StatusPicker

## Caching Layers

Three cache layers avoid redundant work:

1. **expanded_body_cache** (`HashMap<PathBuf, String>`) -- in-memory expanded markdown
2. **disk_cache** (`DiskCache`) -- persistent on disk at `~/.lazyspec/cache/`
3. **diagram_cache** (`DiagramCache`) -- rendered diagram images keyed by source hash

Expansion requests check disk cache first, fall back to spawning a worker thread.
