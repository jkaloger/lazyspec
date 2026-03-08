---
title: "Graph Mode"
type: story
status: accepted
author: "jkaloger"
date: 2026-03-05
tags: [tui]
related:
  - implements: docs/rfcs/RFC-006-tui-progressive-disclosure.md
---


## Context

Documents in lazyspec form a dependency graph through typed relationships (implements, blocks, supersedes, related-to). The Relations tab shows immediate neighbours, but there's no way to see the full picture: which RFCs have stories, which stories have iterations, where chains are broken. Graph mode renders this as a navigable tree, making the project's structure visible at a glance.

## Acceptance Criteria

### AC1: Tree rendering from implements edges

- **Given** the TUI is in Graph mode and documents have `implements` relationships
  **When** the right panel renders
  **Then** documents are displayed as an indented tree using box-drawing characters, with root documents (those with no incoming `implements` links) at the top level

### AC2: Node display

- **Given** a node renders in the graph
  **When** the screen draws
  **Then** each node shows a type indicator, the document title, and status in its status colour

### AC3: Navigate nodes

- **Given** the TUI is in Graph mode
  **When** the user presses `j/k`
  **Then** selection moves between nodes in depth-first order, with the selected node highlighted in cyan/bold

### AC4: Collapse and expand subtrees

- **Given** a node with children is selected
  **When** the user presses `h`
  **Then** the subtree collapses and a collapse indicator is shown
- **Given** a collapsed node is selected
  **When** the user presses `l`
  **Then** the subtree expands

### AC5: Jump to document

- **Given** a node is selected in Graph mode
  **When** the user presses Enter
  **Then** the TUI switches to Types mode with that document's type selected and the document selected in the list

### AC6: Cross-cutting edge annotations

- **Given** a document has `blocks`, `related-to`, or `supersedes` relations
  **When** its node renders in the graph
  **Then** those relations appear as inline annotations after the node (e.g. a dimmed reference to the target document)

### AC7: Legend panel

- **Given** the TUI is in Graph mode
  **When** the left panel renders
  **Then** it shows a legend mapping type indicators to document types and edge styles to relation types

### AC8: Graph filter

- **Given** the TUI is in Graph mode
  **When** the user changes the filter on the left panel
  **Then** the graph shows only trees rooted at documents matching that filter

## Scope

### In Scope

- Tree construction from `implements` edges (forest of trees)
- Depth-first flattening for navigation
- Box-drawing character tree rendering
- Collapse/expand with `h/l`
- Jump to document with Enter
- Cross-cutting edge annotations (inline text, not drawn edges)
- Legend and filter controls on the left panel

### Out of Scope

- Canvas-based graph rendering with drawn edges for cross-cutting relations
- Drag-and-drop or spatial graph layout
- Editing relations from within Graph mode
