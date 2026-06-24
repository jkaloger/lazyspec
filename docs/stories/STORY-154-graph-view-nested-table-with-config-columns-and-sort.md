---
title: Graph view nested table with config columns and sort
type: story
status: accepted
author: jkaloger
date: 2026-06-23
tags: []
related:
- implements: RFC-049
---<!-- intent: define a vertical slice of value with testable acceptance criteria -->

## Context

RFC-049 payoff: render the graph as a nested table (matching the types-view nested doc grammar), with config-declared columns and sibling sort by attribute. Depends on attributes (STORY-150) and the pivot (STORY-153).

## Acceptance Criteria

- **Given** `tui.graph.columns` in config (default `["status","related"]`)
  **When** the graph renders
  **Then** the DOC column keeps tree indentation/connectors and each declared column renders aligned beside it

- **Given** a column naming an attribute not declared on a visible type
  **When** rendering
  **Then** that cell is empty for those rows (no crash)

- **Given** `tui.graph.sort` default and a cycle key (`o` to cycle, `O` to reverse — `s` is taken by status)
  **When** the user cycles sort
  **Then** siblings reorder by the active column within each subtree, ties broken by path (total, stable); the header shows the active column + direction

- **Given** siblings where some lack the sort attribute
  **When** sorted
  **Then** missing values sort last (deterministic)

## Scope

### In Scope

- Nested-table renderer replacing connector-art-only graph render (`panels.rs:1555`)
- `tui.graph.columns` + `tui.graph.sort` config, with defaults
- Sort cycle/reverse keybinds (`o`/`O`) + header indicator
- Sibling-scoped sort comparator with path tiebreaker

### Out of Scope

- Column-header click-to-sort
- Attribute schema (STORY-150), pivot picker (STORY-153)
