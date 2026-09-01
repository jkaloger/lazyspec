---
title: Wildcard edge endpoints with specific-over-wildcard resolution
type: adr
status: accepted
author: Jack Kaloger
date: 2026-08-25
tags: []
related:
- related-to: RFC-067
---
## Context

Per-edge `traversal` removes the blanket that `traversal = "chain"` on a relationship name provides today. Without some form of wildcard, every walking type pair needs its own row and one line becomes N-squared lines. `related-to` is the clearest case: it is meaningful between any two types, and enumerating that is absurd.

So the edge table needs wildcards. That raises two questions the schema must answer, not leave to reader intuition: what does a wildcard mean on each of the three positions, and what happens when a wildcard row and a specific row both match the same concrete edge.

An alternative was to allow no wildcards and accept the row count, relying on generated config. Rejected: a config a human cannot read by hand is not a config, and `init` would emit dozens of rows for a starter project.

## Decision

`from`, `to`, and `via` each accept `"*"`. `via = "*"` is written explicitly; absent `via` is not permitted, because letting absence mean "any relationship" hides a second rule inside the table's shape.

Resolution when several rows match one concrete edge:

- **Traversal composes.** An edge walks if any matching row gives it a traversal role. Two rows assigning *different* roles to the same triple is a load error, not a precedence puzzle.
- **Requiredness resolution ranges only over rows that state `required`.** Among those, the most specific row wins: specificity is ordered by how many of the three positions are concrete, and three beats two beats one beats zero. A tie between rows of equal specificity that state *different* severities is a load error.
- **A row that omits `required` is documentation.** It declares an edge legal and takes no part in requiredness resolution at any specificity. It cannot tie with a demand at load, and it cannot displace one at validation time however specific it is.
- **Requiredness is resolved per document type, not per concrete edge.** A row applies to a document when `from` matches the document's type; the surviving demands are then checked against the document's `related` list. It cannot be a per-edge loop, because requiredness is a claim about *absence*: a document with an empty `related` list has no concrete edge for such a loop to range over, and that document is precisely the one a demand exists to catch.
- **`required` on a wildcard `from` is rejected at load.** It would demand the edge from every declared type, including types where it is nonsense. RFC-067 lists this as an open question; this ADR closes it as a load error.

This supersedes the wording this ADR carried when first accepted, which said requiredness takes the most specific matching row full stop, and that any equal-specificity disagreement — counting an omitted `required` as one of the things that could disagree — is a load error. Two failures forced the change:

- **The recommended starter shape could not load.** RFC-067 §Design pairs a wildcard `related-to` row (`from = "*"`, `to = "*"`, `via = "related-to"`) with a `relation-existence`-shaped demand (`from = T`, `to = "*"`, `via = "*"`, `required = "error"`). Both score one concrete position, both cover `T -related-to-> anything`, and they "disagreed" only because one of them was silent. STORY-258's migration emits exactly both shapes, so it could not produce a loadable config.
- **Silence was displacing demands.** Under the old rule a narrow documentation-only row — `from = "iteration"`, `to = ["story"]`, `via = "implements"`, no `required` — displaced a broad `from = "iteration"`, `to = "*"`, `via = "*"`, `required = "error"`, and an iteration with zero relations silently stopped being a finding. Writing down that a link is legal is not a request to stop checking something else.

Both follow from one rule: a row that says nothing about requiredness says nothing about requiredness. Waiving a broader demand is a real thing to want, but it needs a spelling that says so out loud; this ADR does not give it one.

## Consequences

- The starter config stays short: one wildcard row for `related-to`, one for `targets`, and concrete rows only where a constraint is genuinely wanted.
- Load-time validation grows real work: overlap detection across rows, contradiction detection for traversal roles, and specificity-tie detection. This is a load cost paid once, not a per-walk cost.
- Error messages must name both conflicting rows by `name`, which is why `name` stays a required field on every edge rather than being optional as it could be.
- Specificity by concrete-position count is coarse. Two rows that both score one — `from = "iteration"`, `to = "*"` and `from = "*"`, `to = ["story"]` — tie rather than resolving. Accepted: an explicit error beats a rule nobody can predict. That particular pair no longer errors, though, because only one of them can carry `required` at all: a wildcard `from` may not. A one-position tie now needs both rows to name `from` concretely with intersecting type sets.
- A table can be silent about requiredness in two different ways that read alike but mean the same thing: a row with no `required`, and no row at all. Both leave the demand from a broader row standing. There is deliberately no way to write "legal here, and stop demanding the broader edge" — that spelling is a later decision, not this one.
