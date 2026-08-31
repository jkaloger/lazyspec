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
- **Requiredness and gating take the most specific row.** Specificity is ordered by how many of the three positions are concrete: three beats two beats one beats zero. A tie between rows of equal specificity that disagree is a load error.
- **`required` on a wildcard `from` is rejected at load.** It would demand the edge from every declared type, including types where it is nonsense. RFC-067 lists this as an open question; this ADR closes it as a load error.

## Consequences

- The starter config stays short: one wildcard row for `related-to`, one for `targets`, and concrete rows only where a constraint is genuinely wanted.
- Load-time validation grows real work: overlap detection across rows, contradiction detection for traversal roles, and specificity-tie detection. This is a load cost paid once, not a per-walk cost.
- Error messages must name both conflicting rows by `name`, which is why `name` stays a required field on every edge rather than being optional as it could be.
- Specificity by concrete-position count is coarse. `from = "iteration"`, `to = "*"` and `from = "*"`, `to = ["story"]` both score one and would be a tie error rather than resolving. Accepted: an explicit error beats a rule nobody can predict.
