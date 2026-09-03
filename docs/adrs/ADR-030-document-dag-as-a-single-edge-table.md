---
title: Document DAG as a single edge table
type: adr
status: accepted
author: Jack Kaloger
date: 2026-08-25
tags: []
related:
- related-to: RFC-067
- related-to: ADR-022
- related-to: ADR-010
---
## Context

RFC-067 needs one home for the document DAG. Today it has three: `[[relationships]].traversal` decides which relationship names form the hierarchy walk (`config.rs:418`, consumed at `store.rs:114-123`), `parent-child` rules declare type pairs without naming a relationship (`config.rs:31-38`), and `relation-existence` rules declare a bare "needs any link" constraint. None of the three can express a type set, and none can express a relationship-plus-type-pair together — which is why `iteration --targets--> STORY-250` currently satisfies `iterations-need-stories` (`validation.rs:534-548`).

Options considered, in ascending cost: named type groups (a `work-item` group used wherever a type name is accepted, leaving the schema alone); `parent` widened to a list; edges owned by each type inside `[[types]]`; endpoints attached to `[[relationships]]`; or a standalone edge table.

Groups and a widened `parent` both answer the disjunction ask cheaply and change nothing else — but neither gives an edge an owning row, so the unnamed-relationship defect survives and `require_parent_status` stays ill-defined against a heterogeneous type set. Type-owned edges make the reverse direction (what are my children?) an index build and leave symmetric relationships without a natural owner. Endpoints on `[[relationships]]` keep `traversal` where it is, which is cheap, but strands `relation-existence` and grows deeply nested TOML.

## Decision

A single `[[edges]]` table owns the DAG. `EdgeDef` carries `name`, `from`, `to`, `via`, `traversal`, and `required`. Both `ValidationRule` variants are removed; `traversal` moves off `RelationshipDef`. `[[relationships]]` retains `name`, `inverse`, and `github_native`.

`to` is a type selector, so "an iteration implements a spike, a story, or a bug" is one row. `via` names the relationship, closing the unnamed-relationship hole structurally rather than by adding a field.

`TypeDef.parent_type` is untouched. It means containment (shared store backend, directory nesting) and is documented as such. Folding containment into the edge table would re-conflate the two meanings the RFC set out to separate.

## Consequences

- The `/lazy` and `/execute` boundary derivation, currently "the union of `parent_type` edges and parent-child rules", collapses to one table plus containment.
- `Store.chain_relationships: Vec<String>` (`store.rs:41`) cannot stay a flat name list — the walk needs from/to types at each hop. This cascades to `context.rs`, `graph.rs`, `cli/context.rs`, the TUI graph view, and the web view. It is the largest single cost of this decision and the reason RFC-067 sequences it as its own story.
- Every existing config needs migration; see the migration ADR.
- Supersedes RFC-042 §Design.2's placement of constraints in `[[rules]]`, while honouring its unbuilt intent that constraints reference relationships by name. Its other half stands: relations remain arbitrary doc→doc at the model level, so the edge table informs `validate`, never `link`.
- Amends ADR-022 in carrier only. Status-conditioned gating over a phase axis remains the right call; `require_parent_status` becomes `require_to_status`, a map keyed by target type, because a type set can span lifecycles.

**Amended 2026-08-31 (ADR-033):** the gating consequence above no longer holds. `require_to_status` is dropped from `EdgeDef` and status-conditioned `create` gating is abandoned outright; ADR-033 supersedes ADR-022 rather than amending its carrier. The rest of this decision -- the single edge table, `via` naming the relationship, `to` as a type selector -- stands.
- Per-edge traversal is precise only where a row is spent. Wildcard rows restore blanket behaviour by design; the RFC states this rather than claiming traversal is fully fixed.

**Amended 2026-09-01 (ADR-034):** `from` and `to` are read off the declaration, not off the walk. Whatever traversal role a row assigns, `from` is the type of the document whose frontmatter declared the relation -- so a link read backwards, and a link a nested child inherited from its parent, are both asked as the declaring document's triple. Wildcard rows are unaffected; this only ever mattered where a row names a concrete `from`.
