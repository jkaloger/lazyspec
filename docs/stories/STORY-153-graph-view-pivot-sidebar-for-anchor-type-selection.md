---
title: Graph view pivot sidebar for anchor type selection
type: story
status: accepted
author: jkaloger
date: 2026-06-23
tags: []
related:
- implements: RFC-049
---<!-- intent: define a vertical slice of value with testable acceptance criteria -->

## Context

RFC-049: the graph view's left column is a dead empty " Graph " block (`views.rs:1555`). It becomes a pivot picker reusing the types-view sidebar grammar (`move_type_prev/next`, `selected_type` at `app.rs:441`). Depends on the engine anchor from STORY-151.

## Acceptance Criteria

- **Given** the graph view
  **When** it renders
  **Then** the left column lists the document types (navigable), replacing the empty block

- **Given** the pivot picker
  **When** the user presses `h`/`l`
  **Then** selection moves between types, same grammar as the types view

- **Given** a selected anchor type
  **When** selection changes
  **Then** `rebuild_graph` calls `resolve_forest(store, Some(anchor))` and the graph re-roots; no re-rooting logic lives in the TUI

- **Given** no anchor selected (default)
  **When** the graph renders
  **Then** it shows the whole-store forest as today

## Scope

### In Scope

- `App.graph_anchor` state + `h`/`l` handling in `handle_graph_key`
- Left-column type-list renderer for the graph view
- `rebuild_graph` passes the anchor to the engine
- Keybind registry entry

### Out of Scope

- Engine anchoring (STORY-151)
- Table columns / sort (STORY-154)
