---
title: Context model is a multi-parent DAG, not a linear chain
type: adr
status: draft
author: jkaloger
date: 2026-06-17
tags:
- context
- relationships
related:
- related-to: STORY-122
- related-to: RFC-007
---

## Summary

`lazyspec context` models a document's upward lineage as a multi-parent DAG
resolved by breadth-first traversal over all `implements` relations, replacing
the single-parent linear chain.

## Context

The original `resolve_chain` walks upward by following the *first* `implements`
relation on each document and prepending the parent to a linear `Vec`. The data
model (`DocMeta.related: Vec<Relation>`) and the frontmatter parser both already
permit a document to declare multiple `implements` relations; only the walker
collapses them, silently dropping every parent after the first.

The walk also has no visited-set, so a relationship cycle (A implements B,
B implements A) loops forever.

Agent orchestration (RFC-041) hydrates a builder prompt with the full context
chain. A document that legitimately descends from more than one parent must
surface all of its lineage for that hydration to be complete.

## Decision

- Resolve the upward lineage with BFS over **all** `implements` relations, not
  just the first.
- Maintain a `seen` set keyed by document path. This deduplicates shared
  ancestors and, because a revisited node is skipped, guards against cycles in
  the same mechanism.
- Diamonds (two parents sharing a grandparent) surface the shared ancestor
  exactly once.
- The resolved result is an ancestor **set plus the `implements` edges** between
  members, not an ordered linear chain. The `target_index` (a position in a
  linear list) is replaced by an explicit reference to the target document.
- Human output renders the graph as an indented tree; a single-parent document
  still renders as the existing vertical stack, preserving backward
  compatibility. The target keeps its `<- you are here` marker.

## Consequences

- The `ResolvedContext` shape and the `--json` chain representation change:
  consumers reconstruct the DAG from the ancestor set and edges rather than
  reading an ordered list. This is a breaking change to the JSON contract for
  the `chain` field.
- Multiple `implements` per document is allowed freely; no cardinality
  validation is added.
- The latent infinite-loop on cyclic `implements` is fixed as a side effect.
- Tree rendering of arbitrary DAGs is more involved than a linear stack; shared
  nodes are drawn once, which keeps output bounded but means an edge may point
  to a node rendered elsewhere in the tree.
