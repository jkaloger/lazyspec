---
title: Partial GraphQL responses are the error model
type: adr
status: draft
author: Jack Kaloger
date: 2026-08-06
tags: []
related:
- implements: RFC-065
---## Context

RFC-065 replaces the fetch read layer with one composed GraphQL document per
round. One request is one failure domain: before, a missing `project` scope
failed only the board-field request and left the issue-types request alone.

GitHub does not fail a partly-broken query. It returns HTTP 200 with `data`
filled in for every subtree that resolved, plus an `errors[]` array describing
what did not. Verified live: a nonexistent board alongside a valid issue list
returned the issues intact, `data.repository.owner.bad` null, and
`errors[0].path = ["repository","owner","bad"]`.

An error entry does **not** always carry a `path`. A timed-out or rate-limited
query answers with the failed connections nulled and a pathless entry --
`{"message":"Something went wrong while executing your query..."}` -- because
the failure is the whole response, not one field of it. A `path` is therefore
a narrowing of blame, and its absence is the widest blame, not the least.

`GhCli::graphql` (`gh.rs:1667`) already parses stdout before checking the exit
status and returns `Ok(json)` whenever `data` is present, so `gh`'s non-zero
exit on a partial response is already handled.

## Decision

The `errors[]` array plus the shape of `data` together are the error model for
a fetch round. The parser reads each subtree independently and maps each
failure onto a per-subtree warning, reusing the strings the per-request paths
emitted.

A subtree fails, and so resolves to `None` on `FetchSnapshot` rather than to an
empty collection, when any of these hold:

- an error entry's `path` names it, or names an ancestor of it;
- an error entry carries no `path` at all, which fails every subtree;
- the value is absent or null where GitHub's schema declares it non-null
  (`Repository.milestones` is `MilestoneConnection!`), because only error
  propagation can produce that.

`Some(vec![])` means the repo has none of that thing -- an empty `nodes` array,
which is something the server said. `None` means the round did not learn.
Consumers overwrite their cache on `Some` and keep it on `None`.

Absence is read against the schema, not uniformly. A user-owned repo returns no
`issueTypes` key because the Organization fragment did not apply, and that
absence is the answer: `Some(vec![])`, no warning. A *null* `issueTypes` is
error propagation and stays unknown. The rule is that emptiness must be
something GitHub said, never something a broken read left behind.

A transport failure (timeout, auth, rate limit) folds into the same shape: a
snapshot where every subtree is `None`, carrying one warning.

An unresolved subtree is a failure, not a quiet no-op. A consumer for which the
round is the only source reports an error as well as the warning, so a read
that never happened is never reported as `fetched: 0` on a successful run.

## Consequences

- Per-piece best-effort survives the collapse to one request: a failed board
  still leaves milestones and issue types landing, and board-bound docs keep
  their last known status.
- The three-valued subtree is load-bearing, not stylistic. A `Vec` would make
  "the round failed" indistinguishable from "the repo has none", and an
  authoritative syncer would read that as a deletion and wipe the cache.
- Adding a subtree means deciding, from GitHub's schema, whether its absence is
  an answer or a failure. Nullable-by-schema fields may resolve empty on
  absence; non-null ones may not.
- A pathless error costs the round everything, including subtrees whose data
  looks intact. Losing a healthy subtree to a whole-response error is the
  cheaper mistake: the consumer keeps its prior cache.
- `GhGraphql` and `GhCli::graphql` are unchanged. No transport seam work is
  needed to support partial responses, now or for the remaining RFC-065 slices.
- A `data.repository` that is null or absent fails every subtree, so a
  wholly-failed response cannot be mistaken for an empty repo.
