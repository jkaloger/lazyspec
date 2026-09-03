---
title: An edge is asked with the declaring document's type as from
type: adr
status: accepted
author: jack
date: 2026-09-01
tags: []
related:
- related-to: RFC-067
- related-to: ADR-030
---

## Context

RFC-067 moves traversal onto `[[edges]]` rows carrying `from`, `to`, `via` and `traversal` (ADR-030), so both traversals are decided by asking a triple instead of matching a relationship name. A triple has a direction, and the walks read the same declared link from both ends: `resolve_chain`'s related BFS, the graph view's annotations, and `resolve_chain`'s chain-children hop each stand on one document and look at links pointing at it. On top of that, `Store::propagate_parent_links` lends every link a parent declared to each of its nested children, so a document's link list holds relations it never stated.

Two readings of `from` are therefore available for any link, and under the wildcard rows this project's own config uses they are indistinguishable. They come apart the moment a row names a concrete `from`: a link read backwards, or a link inherited from a parent, is asked as a triple whose source type nobody wrote down, and the neighbour drops out of the CLI, the TUI graph and the web view at once.

## Decision

`from` is the type of the document whose frontmatter declared the relation, and `to` is the type at the far end of that same declaration -- whichever end the walk is standing on, and whichever traversal role the row assigns. The related role is where the ambiguity was found; the rule is not confined to it.

- A link read forwards asks `(subject, via, target)`.
- The same link read backwards asks the same triple, not the reverse of it. Both ends of one declared link are neighbours of each other, or neither is.
- A link a nested child inherited from its parent asks the **parent's** triple. The parent declared it, so the parent's edge is the one that has to admit it.

The store records the declaring document on every link (`Link.declared_by`), because a reader cannot recover it: an inherited link is indistinguishable from an owned one once both sit in the same list. One engine module turns links into neighbours for all three surfaces -- `traversal::related_neighbours` for the neighbourhood, `traversal::chain_children` for the one chain hop that reads the link maps -- so no surface and no other module re-derives the direction.

## Consequences

- A concrete-`from` row means what it says. `from = "story"` admits the links stories declare, from either end, including on the nested children that inherit them. Wildcard configs are unaffected, which is why the ambiguity survived unnoticed until a row spent a concrete `from`.
- Asking with the inheriting child's type becomes unsayable, and that is a real loss: a config cannot express "iterations may inherit their story's mentions" apart from "stories may mention RFCs". Nobody has asked for it, and the spelling would need a fourth position on the row.
- `Link` grows a third field, so every link costs another `PathBuf` and the two link maps are no longer plain `(relationship, path)` tuples. Their readers must now say which end of a link they mean.
- The chain walk is covered too. It sources a document's PARENTS from that document's own `related`, so an inherited edge never becomes the inheritor's own hierarchy; the one hop that reads `reverse_links` -- a target's chain children -- asks the declaring document's triple, the same rule under the same `from`. Asking the inheritor's type there dropped a nested child out of `context`'s `forward` under a concrete-`from` chain row, where a blanket marker kept it, so STORY-257's AC1/AC2 decide this hop and it is not STORY-259's to settle.
- The chain relation is therefore asymmetric in membership, deliberately. A target lists a nested child among its chain children, while that child's own chain does not list the target among its parents. Propagation is annotation, not adoption; this decision fixes which triple is asked, never which list a document lands on.

## Revisit when

- Propagation stops meaning inheritance. If a child's copy of a parent's link ever becomes a link of the child's own -- nested documents promoted to first-class instead of annotations on their parent -- then the child is the declaring document and this rule inverts.
- A row gains a way to name the document a link is read *from* separately from the one that declared it, such as a fourth position or an explicit inherited-edge selector. That would make both readings expressible at once, which is the thing this decision currently forecloses.
