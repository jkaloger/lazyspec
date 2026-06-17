---
title: Multi-parent context graph and depth-N hydration
type: story
status: draft
author: jkaloger
date: 2026-06-17
tags:
- cli
- context
- relationships
- agents
related:
- implements: RFC-007
- related-to: RFC-041
---

## Context

`lazyspec context` resolves a document's lineage for agents and humans. Today it
walks the `implements` chain upward as a linear list (RFC -> Story -> Iteration),
surfaces direct forward implementors, and collects depth-1 `related-to` records
from the chain (STORY-019, STORY-054; specified in SPEC-012).

Three gaps block its use as the hydration source for agent orchestration
(RFC-041, whose builder prompt injects `doc.context_chain`):

1. **Single-parent assumption.** The upward walk follows only the *first*
   `implements` relation, so a document that implements more than one parent has
   its other lineages silently dropped. The data model and parser already permit
   multiple `implements` relations; only the chain walker collapses them. The
   same walk has no cycle guard, so a relationship cycle hangs the command.

2. **Shallow related traversal.** Related records are collected one hop from the
   chain. A document related to a record that is itself related to the chain
   (e.g. an ADR related to an RFC the Story relates to) never surfaces, even when
   a planning agent needs those constraining decisions.

3. **JSON drops forward context.** `--json` emits the backward chain and related
   records but omits forward implementors, which appear only in human output.
   Agents consuming `--json` get a strictly smaller view than humans, violating
   the principle that agents consume the same interface as humans.

This story extends `context` to a multi-parent DAG with configurable-depth
related traversal, and brings `--json` to parity with human output.

## Acceptance Criteria

### Multi-parent DAG chain

- **Given** a document that declares `implements` against more than one parent
  **When** `lazyspec context <id>` is run
  **Then** every ancestor reachable upward via `implements` appears, not only the
  first parent's lineage

- **Given** two parents that share a common ancestor (a diamond)
  **When** `lazyspec context <id>` is run
  **Then** the shared ancestor appears exactly once

- **Given** a relationship cycle (A implements B, B implements A)
  **When** `lazyspec context <id>` is run
  **Then** the command terminates without hanging and each document appears once

- **Given** a single-parent document
  **When** `lazyspec context <id>` is run without `--json`
  **Then** the output renders as the existing vertical stack of mini-cards
  (backward compatible)

- **Given** a multi-parent document
  **When** `lazyspec context <id>` is run without `--json`
  **Then** ancestors render as an indented tree, the target document carries the
  `<- you are here` marker, and a shared ancestor is drawn once

- **Given** a multi-parent document
  **When** `lazyspec context <id> --json` is run
  **Then** the output represents the deduplicated ancestor set plus the
  `implements` edges between ancestors, so a consumer can reconstruct the DAG

### JSON forward parity

- **Given** a document that other documents implement
  **When** `lazyspec context <id> --json` is run
  **Then** the output includes a `forward` array listing those implementors as
  frontmatter objects, matching the forward set shown in human output

- **Given** a document that nothing implements
  **When** `lazyspec context <id> --json` is run
  **Then** the `forward` key is present as an empty array (stable schema)

### Depth-N related traversal and tagging

- **Given** no `--depth` flag
  **When** `lazyspec context <id>` is run
  **Then** related records match the current depth-1 behavior (backward
  compatible default)

- **Given** `--depth 2` and an ADR related to an RFC that a chain document
  relates to
  **When** `lazyspec context <id> --depth 2` is run
  **Then** the ADR appears in the related set

- **Given** `--depth N`
  **When** `lazyspec context <id> --depth N` is run
  **Then** documents reachable within N hops along `related-to` links appear,
  deduplicated, and none beyond N hops appear

- **Given** any surfaced related or forward document
  **When** `lazyspec context <id> --json` is run
  **Then** each carries `relation` (the link type), `distance` (hop count from
  the chain), and `via` (the path it was reached through)

## Design Decisions

Locked decisions emitted as ADRs (see linked records):

- **DAG over linear chain.** The context model becomes a multi-parent DAG:
  BFS upward over all `implements` relations with a dedup seen-set (which also
  guards cycles), output as an ancestor set plus edges, diamonds rendered once,
  `target_index` replaced by an explicit target reference.
- **Depth-bounded related traversal with distance tagging.** A `--depth N` flag
  (default 1) bounds a BFS over `related-to` links; each surfaced document is
  tagged with `relation`, `distance`, and `via`.

## Scope

### In Scope

- BFS upward `implements` traversal over multiple parents, deduplicated, cycle-safe
- `ResolvedContext` shape change: ancestor set + edges instead of linear chain + `target_index`
- Indented-tree human rendering for multi-parent graphs; diamonds drawn once; single-parent stays a stack
- `forward` key in `--json` output (parity with human output)
- `--depth N` flag bounding `related-to` traversal (default 1 = current behavior)
- `{relation, distance, via}` tags on surfaced related/forward documents in `--json`
- Updating SPEC-012 to describe the new behavior

### Out of Scope

- `resolve-context` skill role-based hydration (planning vs build depth policy) -- follow-up
- Fidelity tiering (summary vs full-text injection by distance) -- skill concern, not the engine
- Cross-backend relationship resolution (ITERATION-128, separate draft)
- Cardinality validation restricting multiple `implements` per document -- multi-parent is allowed freely
