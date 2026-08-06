---
title: Nested connections are capped below GitHub's node budget, and truncation warns
type: adr
status: review
author: Jack Kaloger
date: 2026-08-06
tags: []
related:
- implements: RFC-065
---

## Context

GitHub rejects a GraphQL query whose *possible* node count exceeds 500,000, and
computes that figure from the `first:` arguments the document declares rather
than from what comes back. Nesting multiplies: `projectItems(first: N)` with
`fieldValues(first: M)` declares `N + N*M` nodes on every issue of every page of
every type.

Measured against the live API:

```
1 type,  subIssues(100) blockedBy(100) projectItems(50) fieldValues(50) -> OK
2 types, same selection -> "requests up to 550,200 possible nodes which
                            exceeds the maximum limit of 500,000"
```

`projectItems(50) x fieldValues(50)` is 2,550 of those 2,750 nodes per issue. So
the selection that reads a single type comfortably cannot read two, and a
composed document that assumed one request always fits would fail on the tenth
type a user configured rather than on the first.

The per-request reads this replaces had no such ceiling: each asked for one
issue and could take `projectItems(first: 50)` without arithmetic.

## Decision

Every nested connection is capped well below the budget, and the budget is
arithmetic over those caps rather than an assumption.

`subIssues(50) blockedBy(50) projectItems(10) fieldValues(25)`, plus the flat
`labels(20)` and `assignees(10)`, is 390 possible nodes per issue -- 39,100 per
100-issue type alias. `gh_fetch::possible_nodes` computes that from the caps
themselves, and `types_per_request` divides it into the budget: 12 aliases per
document. A project with more types splits across `ceil(T/12)` requests, each
chunk paging independently and merging into one `FetchSnapshot`, with only the
first carrying the repository-wide subtrees. The arithmetic reads the same
constants the query builder does, so a cap cannot be changed without the split
following it.

The caps are lower than the per-request reads', so truncation is now possible
where it was not. It must never be silent: every capped connection selects
`pageInfo { hasNextPage }`, and a `true` becomes a `Truncation` in the snapshot
that `IssueCache::fetch_all` renders as a warning naming the document and the
connection. `fieldValues` overflowing on a board item is reported against the
issue, not the item: the item id is a Projects v2 internal, and the document is
what a reader can go and look at.

## Consequences

An issue with more than 50 sub-issues or 50 blockers, on more than 10 boards, or
with more than 25 field cells on one board item, loses the remainder. No
observed project reaches any of those, and the loss is reported through the same
`--json` warnings every other fetch failure uses, so it surfaces rather than
corrupting a cache quietly.

The escape hatch, if a project ever needs it, is a per-issue follow-up query for
the overflowing connections only. It is deliberately not built: it would cost a
request per affected issue, which is what this whole change removes, for a case
that does not currently exist.

Raising a cap is not free and is not a local edit. It changes `possible_nodes`,
which changes how many types fit one request, which changes the request count
for every large project. That coupling is the point -- it is what stops a cap
being raised without the budget being reconsidered.

A type cannot be split, so a single type's alias must always fit: at 39,100
nodes it uses under a tenth of the budget, and the arithmetic floors at one type
per request regardless.

