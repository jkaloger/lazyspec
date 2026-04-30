---
title: TUI sequencing screen render and scope filter
type: story
status: draft
author: jkaloger
date: 2026-04-30
tags: []
related:
- implements: RFC-041
- blocks: STORY-119
- blocks: STORY-117
- blocks: STORY-115
---





## Summary

Introduce a new TUI sequencing screen that renders the full `blocks` + `implements` DAG across project documents, replacing the rendering path used by the prior graph mode. The screen is read-only in this slice: it lays out every document as a node, distinguishes the two edge types visually, colours nodes by status, and supports a scope filter that highlights an `under <id>` or `after <id>` subset against the dimmed remainder of the project for orientation. Edge mutation, priority editing, budget panel, and critical-path overlay are deferred to later slices.

## In Scope

- A new TUI screen that renders the full document DAG using the existing layered layout primitives.
- Visual distinction between `blocks` edges and `implements` edges.
- Node colouring driven by document status.
- Scope filter with three modes: whole project, `under <id>`, `after <id>`.
- Dimmed rendering for non-scope nodes; highlighted rendering for in-scope nodes when a scope is active.
- Iteration ids rejected as a `--scope` source, consistent with the CLI rule that scope only accepts documents with implements-descendants.
- Read-only behaviour: the screen renders and filters, nothing else.

## Out of Scope

- Adding or removing `blocks` edges from the screen (later slice).
- Priority editing, budget panel, and critical-path overlay (later slice).
- Removing the existing graph-mode wiring from the prior story (later slice).
- The CLI `graph` command (separate slice).
- Engine graph internals: cycle check, topo order, ready traversal (separate slice).

## Acceptance Criteria

- **Given** a project with documents connected by both `blocks` and `implements` relationships
  **When** the user opens the sequencing screen with no scope set
  **Then** every document appears as a node and both edge types are rendered with visually distinct styling so the user can tell them apart at a glance.

- **Given** the sequencing screen is open on the whole project
  **When** the screen renders
  **Then** each node's colour reflects its document status, with different statuses rendered in distinguishable colours.

- **Given** the user wants to focus on a single RFC or Story
  **When** they apply scope mode `under <id>`
  **Then** the implements-descendants and transitive blocks-ancestors of that document are highlighted, and all remaining documents are rendered dimmed but still visible for orientation.

- **Given** the user wants to see what a document unlocks
  **When** they apply scope mode `after <id>`
  **Then** the transitive blocks-descendants of that document are highlighted and the remaining documents are rendered dimmed.

- **Given** no scope is set
  **When** the screen renders
  **Then** all nodes are rendered in their normal (non-dimmed) state.

- **Given** the user attempts to set the scope to an iteration id
  **When** they submit that selection
  **Then** the screen rejects the choice with a visible message indicating iterations are not valid scope sources, and the previous scope state is preserved.

- **Given** the sequencing screen is open in any scope mode
  **When** the user attempts any edit gesture (add edge, remove edge, change priority)
  **Then** no document on disk is modified, because the screen is read-only in this slice.
