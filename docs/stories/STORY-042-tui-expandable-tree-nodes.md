---
title: TUI expandable tree nodes
type: story
status: accepted
author: jkaloger
date: 2026-03-07
tags: []
related:
- implements: docs/rfcs/RFC-014-nested-child-document-support.md
---



## Context

Folder-based documents with children need a way to be browsed in the TUI. The graph mode already implements depth-based tree rendering with ASCII connectors and expand/collapse navigation. This story extends the document list view so that parent documents with children render as expandable tree nodes, reusing the patterns established in graph mode.

## Acceptance Criteria

### AC1: Parent documents show as expandable nodes

- **Given** the TUI is open and a parent document has child documents
  **When** the document list is rendered
  **Then** the parent appears with a visual indicator that it can be expanded (e.g. a collapse/expand marker)

### AC2: Expand reveals children

- **Given** a collapsed parent document is selected in the document list
  **When** the user presses the expand key
  **Then** child documents appear as indented items beneath the parent

### AC3: Collapse hides children

- **Given** an expanded parent document is selected
  **When** the user presses the collapse key
  **Then** child documents are hidden and the parent returns to its collapsed state

### AC4: Navigation into children

- **Given** a parent is expanded and its children are visible
  **When** the user navigates down
  **Then** child documents are selectable and the preview pane shows the selected child's content

### AC5: Virtual parents render correctly

- **Given** a virtual parent (no index.md) has been synthesised
  **When** it appears in the TUI document list
  **Then** it displays with its derived title and a visual indicator that it is a virtual (non-file) document

## Scope

### In Scope

- Expandable/collapsible parent nodes in the document list view
- Indented child rendering reusing depth-based patterns from graph mode
- Navigation into and out of child documents
- Visual distinction for virtual parents

### Out of Scope

- Engine discovery changes (STORY-040)
- CLI command output (STORY-041)
- Graph mode changes (parent-child from folder structure is a list-view concern; graph mode already handles `implements` relationships)
