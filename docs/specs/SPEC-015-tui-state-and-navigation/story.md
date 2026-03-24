---
title: "TUI State and Navigation"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: [tui, state, navigation]
related:
  - implements: docs/architecture/ARCH-005-tui/app-state.md
---

## Acceptance Criteria

### AC: modal-priority-chain

Given the create form is active and the help overlay is also shown
When the user presses any key
Then the help overlay is dismissed first, and the create form does not receive the keypress

### AC: mode-cycle-order

Given the TUI is in Types mode
When the user presses backtick repeatedly
Then the mode cycles through Types, Filters, Metrics, Graph (and Agents if the agent feature is enabled) before returning to Types

### AC: mode-transition-side-effects

Given the TUI is in Filters mode with an active status filter
When the user presses backtick to leave Filters mode
Then the active filters are reset before the mode transitions

### AC: doc-tree-parent-child-structure

Given a document type contains parent documents with children
When the document tree is built
Then top-level documents appear at depth 0, and children of expanded parents appear at depth 1 sorted by date

### AC: expand-collapse-toggle

Given a collapsed parent document is selected in the document list
When the user presses space
Then the parent's children become visible in the tree, and pressing space again on the parent hides them and clamps the selection index

### AC: collapse-from-child

Given an expanded parent's child is selected
When the user presses space
Then the selection moves to the parent and the parent collapses

### AC: type-switching-resets-selection

Given the user is viewing a document type with a non-zero selected_doc
When the user presses h or l to switch document types
Then selected_doc resets to 0 and the document tree rebuilds for the new type

### AC: scroll-padding-enforcement

Given the document list has more items than the visible height
When the user navigates with j/k such that selected_doc is within 2 items of the viewport edge
Then the viewport offset adjusts to maintain at least 2 lines of padding between the selection and the viewport boundary

### AC: half-page-navigation

Given the document list height is 20
When the user presses Ctrl-d
Then selected_doc advances by 10 and the viewport adjusts accordingly

### AC: fullscreen-scroll-independence

Given a document is open in fullscreen mode
When the user presses j or k
Then scroll_offset changes by 1, and Ctrl-d/Ctrl-u jumps by half of fullscreen_height

### AC: graph-dfs-no-cycles

Given the document store contains a circular implements chain (A implements B, B implements A)
When the graph is rebuilt
Then each document appears at most once in graph_nodes due to the visited set

### AC: graph-enter-navigates-to-types

Given the user is in Graph mode with a node selected
When the user presses Enter
Then the view switches to Types mode, the correct document type is selected, and the corresponding document is highlighted in the tree

### AC: search-mode-consumes-input

Given search mode is active
When the user types characters
Then the characters append to search_query and results update, rather than triggering normal-mode keybindings

### AC: filter-field-cycling

Given the TUI is in Filters mode with the Status field focused
When the user presses Tab
Then focus moves to Tag, and pressing Tab again moves to ClearAction, then back to Status
