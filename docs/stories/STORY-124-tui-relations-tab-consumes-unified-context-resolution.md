---
title: TUI relations tab consumes unified context resolution
type: story
status: draft
author: jkaloger
date: 2026-06-18
tags:
- tui
- relations
- context
related:
- implements: RFC-005
- related-to: STORY-010
- related-to: SPEC-012
- related-to: STORY-122
---

## Context

The TUI relations tab (`render_relationship_sections` in `src/tui/views/panels.rs`)
renders a selected document's relationships. To draw the lineage it walks its own
`implements` chain, selecting a single parent per hop via `find_map`, so a document that
`implements` more than one parent shows only the first. This is a third copy of
relationship-graph traversal, separate from both the `lazyspec context` command and the
graph view, and it diverges from them in the same way: multi-parent lineage collapses.

The shared resolution logic is being lifted into an engine module (a separate,
behaviour-preserving refactor iteration) that exposes a single-target neighbourhood walk:
the upward ancestor DAG, forward implementors, and the `related-to` set. The relations
tab should build its sections from that walk on the selected document, so the tab, the
graph view, and the CLI all present the same relationships for a given document.

This story covers only the relations tab's adoption of the engine module. The engine
extraction itself, the CLI, and the graph view are out of scope (see Scope).

## Acceptance Criteria

- **Given** the selected document `implements` two parent documents
  **When** the relations tab renders
  **Then** both parents appear in the lineage, rather than only the first.

- **Given** the selected document has `related-to` neighbours, including those reachable
  through an inverse relationship alias
  **When** the relations tab renders
  **Then** the related set shown matches the related set the `context` command reports for
  that document.

- **Given** the engine context module is available
  **When** the relations tab builds its sections
  **Then** it obtains the lineage, forward implementors, and related set from the engine's
  single-target resolution of the selected document, and the tab-local `implements` chain
  walk is removed.

- **Given** the selected document has no relationships
  **When** the relations tab renders
  **Then** the "No relations." empty state is shown, unchanged from current behaviour.

- **Given** a document with only single-parent `implements` lineage and direct
  `related-to` links
  **When** the relations tab renders
  **Then** the displayed sections are unchanged from the previous behaviour (backward
  compatible for the common case).

## Scope

### In Scope

- The relations tab consuming the engine context module's single-target resolution for the
  selected document.
- Showing all parents of a multi-parent document in the lineage.
- A related set consistent with the `context` command.
- Removing the tab-local `implements` chain walk in `render_relationship_sections`.
- Preserving the empty state and single-parent presentation.

### Out of Scope

- Extracting the resolution logic into the engine (separate behaviour-preserving refactor
  iteration; this story depends on it).
- Any change to `lazyspec context` CLI output.
- The TUI graph view (STORY-123).
- The link editor and link/unlink interactions.
- A depth-N control for `related-to` in the TUI; the relations tab uses the default depth.
