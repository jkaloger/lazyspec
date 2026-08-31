---
title: Wildcard edge endpoints for a short starter config
type: story
status: accepted
author: Jack Kaloger
date: 2026-08-29
tags: []
related:
- implements: RFC-067
- blocks: STORY-258
---
As a DAG designer, I want `"*"` accepted on an edge's endpoints, so that a relationship meaningful between any two types does not need one row per pair.

Per-edge traversal removes the blanket that a relationship-level `traversal` flag provides today. Without wildcards, `related-to` alone would need N-squared rows and `init` would emit an unreadable starter config.

## Acceptance criteria

- Given an edge `from = "*"`, `to = "*"`, `via = "related-to"`, when any two documents link `related-to`, then that edge matches.
- Given a wildcard row and a concrete row that both match one link, when `validate` runs, then requiredness and gating come from the concrete row — specificity is ordered by count of concrete positions among `from`/`to`/`via`.
- Given two matching rows of equal specificity that disagree on requiredness, when the config loads, then load fails naming both rows by `name`. An explicit error is preferred over an unpredictable precedence rule (ADR-031).
- Given `required` set on a row whose `from` is `"*"`, when the config loads, then load fails — it would demand the edge from every declared type.
- Given an edge with `via` absent entirely, when the config loads, then load fails asking for an explicit `via`, including `via = "*"`. Absence must not silently mean "any relationship".
- Given `via = "*"`, `to = "*"`, `required = "error"`, when a document of `from`'s type carries no relation at all, then `validate` reports the error — this is the shape `relation-existence` translates to.

## Notes

Depends on STORY-254's loader. Error messages must name both conflicting rows, which is why `name` stays mandatory on every edge rather than optional.

Overlap and contradiction detection is a load-time cost paid once, not a per-walk cost.
