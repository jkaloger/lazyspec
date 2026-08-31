---
title: Walk the document DAG from the edge table
type: story
status: accepted
author: Jack Kaloger
date: 2026-08-29
tags: []
related:
- implements: RFC-067
- blocks: STORY-262
---
As a document author, I want `context` to follow only the edges declared to walk, so that a relationship used for one purpose stops implying hierarchy everywhere else.

Today `traversal` is a global property of a relationship name (`src/engine/config.rs:418`). This project marks `targets` as `chain`, which is genuine hierarchy for iteration→milestone and accidental hierarchy for every other pair. Moving traversal onto the edge makes precision available per pair.

The largest slice in RFC-067. If it proves too big once the index shape is known, split it by surface — engine and CLI first, then TUI graph and web view.

## Acceptance criteria

- Given an edge `from = "iteration"`, `to = ["milestone"]`, `via = "targets"`, `traversal = "chain"` and no other `targets` edge, when `context` runs on a story that a document targets, then that document does not appear in the story's chain.
- Given the same config, when `context` runs on an iteration targeting a milestone, then the milestone appears in the chain.
- Given two matching rows assigning different traversal roles to one triple, when the config loads, then load fails naming both rows.
- Given a wildcard `related-to` edge with `traversal = "related"`, when `context` runs, then the related neighbourhood matches today's output for the same documents.
- Given any config migrated by `fix --config`, when `context` runs before and after migration, then the output is identical. Migration is behaviour-preserving.
- Given the same document, when viewed via `context --json`, the TUI graph view, and the web view, then all three render the same chain and neighbourhood.

## Notes

`Store.chain_relationships: Vec<String>` (`src/engine/store.rs:41`) cannot survive as a flat name list — the walk needs the from-type and to-type at each hop, not just the relationship name. Whether the reverse index is built eagerly in `Store` or computed per call is an implementation decision for the iteration.

`prompt.rs:177 child_types_for` currently scans rules for `parent == doc_type` and needs the same reverse index.

Cascades to `context.rs`, `graph.rs`, `cli/context.rs`, the TUI graph view, and the web view. Surface parity is an acceptance criterion, not a follow-up.
