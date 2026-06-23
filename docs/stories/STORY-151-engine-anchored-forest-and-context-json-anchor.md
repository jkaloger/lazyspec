---
title: Engine anchored forest and context --json --anchor
type: story
status: accepted
author: jkaloger
date: 2026-06-23
tags: []
related:
- implements: RFC-049
---<!-- intent: define a vertical slice of value with testable acceptance criteria -->

## Context

RFC-049: graph re-rooting must live in the engine, not the TUI (dictum 3), and be reachable by agents (principle 2). Today `resolve_forest` (`context.rs:171`) emits the whole-store forest with all DAG roots, lineage hardcoded to `implements`. There is no way to anchor on a type.

## Acceptance Criteria

- **Given** an anchor type
  **When** `resolve_forest(store, anchor: Option<&str>)` is called with `Some(type)`
  **Then** roots = all docs of that type, each followed by its `implements`-descendant subtree; docs above the anchor are excluded

- **Given** no anchor
  **When** `resolve_forest(store, None)` is called
  **Then** behaviour is identical to today (all DAG roots)

- **Given** `--anchor <type>`
  **When** `lazyspec context --json --anchor story` runs
  **Then** it emits the anchored forest; without `--anchor` it emits the whole-store forest

- **Given** a diamond (a doc with two anchor-type ancestors)
  **When** anchored
  **Then** the doc appears under each anchor root without infinite recursion (existing cycle/seen-set guard holds)

## Scope

### In Scope

- `resolve_forest` anchor parameter + descendant pruning
- `context --json --anchor <type>` CLI flag + help + README
- Reuse existing `topo_order` / cycle handling

### Out of Scope

- Lineage relation parameterisation (stays `implements`)
- TUI consumption (STORY-153)
