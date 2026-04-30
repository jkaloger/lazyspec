---
title: TUI sequencing priority budget and critical-path overlay
type: story
status: draft
author: jkaloger
date: 2026-04-30
tags: []
related:
- implements: RFC-041
---


## Summary

Layer priority editing, a budget panel, and a critical-path overlay onto the
new TUI sequencing screen. Priority is set on the selected node via numeric
keys mapped to the priority slots configured in `lazyspec.toml` (definition
order). The budget panel summarises priority counts across the in-scope
document set, split by `done` vs `remaining`. A critical-path overlay toggle
highlights the longest weighted path through the in-scope DAG and recolours
nodes by priority.

## Scope

### In Scope

- Numeric keys `1`..`9` set the selected node's `priority` field, mapped from
  `lazyspec.toml` `[priorities.*]` definition order.
- Graceful degradation when fewer than 9 priorities are configured (extra keys
  no-op).
- Avoiding collisions with reserved keybinds (`p` reserved for the provenance
  editor).
- Budget panel showing cumulative priority counts in scope, split by `done` vs
  `remaining`.
- Critical-path overlay toggle highlighting the longest weighted path through
  the in-scope DAG.
- Node colouring by `priority`.

### Out of Scope

- Sequencing screen render and scope filter (Story 4a, prerequisite).
- Add/remove `blocks` edge operations (Story 4b, prerequisite).
- `priority` frontmatter field and TOML config (Story 2, dependency).
- Engine `critical_path` implementation (Story 1, dependency).
- CLI surfaces (Story 3).
- Custom per-priority colour configuration (deferred per RFC).

## Acceptance Criteria

- **Given** a node is selected on the sequencing screen and `lazyspec.toml`
  defines priorities `must`, `should`, `could`, `wont`
  **When** the user presses numeric key `2`
  **Then** the selected node's `priority` is set to the second configured
  priority (`should`) and persisted via the engine update path.

- **Given** `lazyspec.toml` defines four priorities and a node is selected
  **When** the user presses numeric key `7`
  **Then** the priority is unchanged and the screen no-ops.

- **Given** the sequencing screen is active and `p` is bound to the provenance
  editor
  **When** the user presses `p`
  **Then** the provenance editor opens and no `priority` change occurs.

- **Given** an in-scope set of nodes with mixed priorities
  **When** the screen renders
  **Then** each node is coloured according to its `priority` value.

- **Given** a node currently rendered in its old priority colour
  **When** the user changes its priority via a numeric key
  **Then** the node's colour updates to reflect the new priority.

- **Given** the budget panel is visible with an in-scope set
  **When** the screen renders
  **Then** the panel shows a count for each configured priority, split into
  `done` and `remaining`, and the totals match the scoped document set.

- **Given** a node in scope transitions to a terminal status
  **When** the budget panel re-renders
  **Then** that node moves from the `remaining` count to the `done` count for
  its priority.

- **Given** the in-scope DAG has a defined critical path
  **When** the user toggles the critical-path overlay on
  **Then** the nodes and edges along the engine's `critical_path` output for
  the current scope are highlighted.

- **Given** the critical-path overlay is on
  **When** the user toggles it off
  **Then** the highlight is removed and the screen returns to its prior
  rendering.

- **Given** the critical-path overlay is on and the user changes the priority
  of a node on the path
  **When** the engine recomputes `critical_path`
  **Then** the highlighted path matches the new engine output.
