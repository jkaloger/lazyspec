---
title: Pagination composes across types per round
type: adr
status: draft
author: Jack Kaloger
date: 2026-08-06
tags: []
related:
- implements: RFC-065
---

## Context

GraphQL connections page at 100. `gh issue list --limit` did not, so the fetch
path read up to `FETCH_LIMIT = 500` issues per type in one REST call and warned
when it came back with exactly 500 -- "there may be more". Moving discovery onto
`repository.issues` would have cut that ceiling from 500 to 100 unless the round
paged, so pagination had to ship with the cut-over rather than after it.

Paging per type is the obvious shape and the wrong one: ten types would cost the
sum of their page counts, which is the per-type request count the composed
document exists to remove. The types are independent connections in one
document, though, and each reports its own `pageInfo`, so a round can carry as
many cursors as it has aliases.

## Decision

A fetch is a sequence of rounds, not a sequence of types. Round one composes one
`t<i>: issues(first: 100, after: $c<i>)` alias per configured type alongside the
repository-wide subtrees. Every type whose alias reports `hasNextPage` has its
`endCursor` bound into the next round, which re-composes only those aliases and
drops the milestone and owner selections it has already answered.

Requests therefore total the largest type's page count, not the sum across
types: one 300-issue type beside nine short ones costs three rounds, not twelve.
The loop lives in `gh_fetch::fetch_all_pages`, above the single-round primitive
and below every caller, so `sync_all` and the TTL refresh page identically.

Because the fetch is now complete, `FETCH_LIMIT` and its truncation warning are
deleted rather than raised. A bounded fetch was an artifact of the transport.

## Consequences

A type with more than 500 issues stops silently missing documents, and the
`fetch --json` warning list loses both retired truncation warnings.

The first fetch of a large repo is slower in wall time than the capped one was,
bounded by pages rather than by a constant. The steady-state poll is unaffected:
a type under 100 issues still costs the one round it always did.

Merging is per type and all-or-nothing. A type whose alias fails in any round is
dropped from the snapshot entirely rather than left holding the pages that did
arrive, because a full fetch rebuilds the type's directory -- half a list would
prune the other half away. `IssueCache::fetch_all` reads that absence as an
error and leaves the prior cache standing (ADR-025's posture, at round
granularity).

A server that offers another page while handing back the cursor it was given
would spin the loop forever, so only a cursor that moved counts as another page.
