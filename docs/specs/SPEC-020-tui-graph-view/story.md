---
title: "TUI Graph View"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: []
related: []
---

## Acceptance Criteria

### AC: root-document-identification

Given the store contains documents with and without `implements` relations
When the graph rebuilds
Then only documents whose own `related` list has no `implements` entry appear at depth 0

### AC: root-sorting

Given multiple root documents exist
When the graph rebuilds
Then roots are sorted by doc type name lexicographically, then by title within each type

### AC: graph-dfs-no-cycles

Given the store contains documents with circular `implements` references
When the graph rebuilds via `traverse_dependency_chain`
Then each document appears at most once and the traversal terminates

### AC: child-sorting

Given a document has multiple children via reverse `implements` links
When the graph rebuilds
Then those children are sorted alphabetically by title

### AC: tree-depth-rendering

Given a graph node has depth > 0
When `draw_graph` renders the node
Then the node is indented with three spaces per ancestor level and prefixed with a box-drawing connector (`├─▶` or `└─▶` depending on sibling position)

### AC: node-display

Given a graph node renders
When the screen draws
Then the node shows a type icon, the document title, and the status string coloured by status

### AC: node-navigation

Given the TUI is in Graph mode with nodes present
When the user presses `j`/`k` (or arrow keys)
Then selection moves up or down within the flat node list, clamped to bounds

### AC: graph-enter-navigates-to-types

Given a node is selected in Graph mode
When the user presses Enter
Then the TUI switches to Types mode with the node's document type selected and the document highlighted in the list

### AC: jump-to-extremes

Given the TUI is in Graph mode
When the user presses `g` or `G`
Then selection jumps to the first or last node respectively

### AC: open-editor

Given a node is selected in Graph mode
When the user presses `e`
Then the selected node's document opens in the external editor
