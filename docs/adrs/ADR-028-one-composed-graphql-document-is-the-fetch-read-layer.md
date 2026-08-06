---
title: One composed GraphQL document is the fetch read layer
type: adr
status: review
author: Jack Kaloger
date: 2026-08-06
tags: []
related:
- implements: RFC-065
---

## Context

A fetch's cost is its request count, not its payload. Measured with a `gh` shim
over a real fetch: a trivial `gh api graphql` round trip is ~0.58s regardless of
what it asks for, and a 10-type project paid ~51 requests -- one REST issue list
per type, one `issue_view` per issue for a type classified by native issue type,
one milestone list, one schema probe per type, one board-fields probe per
authority board, and three `nodes(ids:)` enrichment batches.

The commit before this chunk had already taken blocked-by, sub-issue parentage
and project fields from `O(N)` to `O(N/100)` by batching them through
`nodes(ids:)`. That is the ceiling of that approach rather than a step toward
something better: `nodes(ids:)` needs the ids before it can ask, so it is
structurally a *second* round trip after discovery. No amount of batching
removes the trip; it only makes it cheaper.

GraphQL exposes `subIssues`, `blockedBy` and `projectItems` directly on `Issue`,
and `milestones`, `issueTypes` and `projectV2(number:)` on the same
`repository`/`owner` the issues hang off. So the ids need never leave the server
at all.

## Decision

One composed GraphQL document per fetch round is the whole read layer. Each
`github-issues` type becomes a `t<i>: issues(...)` alias on `repository`, its
discovery rule mapped onto `labels:` plus a client-side `issueType` filter;
sub-issue edges, blocked-by edges and board memberships with their field cells
are inline selections on each issue node; milestones and the owner subtree
(issue types, every authority board's field schema) hang off the same request.

`src/engine/gh_fetch.rs` builds that document from config and parses the
response into a `FetchSnapshot`. It touches no cache and holds no state: the
builder and the parser are the module. `GhGraphql` is unchanged -- this is about
how many times the transport is invoked, not about the transport.

Every read the fetch path used to make is deleted rather than left as a fallback:
`issue_list`, `milestone_list`, `search_issue_numbers_by_type`, the schema
probes, and all three `nodes(ids:)` batches with their queries, their parsers,
`GH_NODES_BATCH_MAX` and the wrappers that existed only to serve a batch to a
per-doc consumer. The retained single-doc reads (`issue_view`, `project_items`,
`list_blocked_by`) belong to the mutation path, where one document genuinely is
one request.

## Consequences

Request count becomes a function of pages, not of types, issues, boards or
enrichment steps. Enrichment costs nothing: a 10-type project with parentage,
dependencies and board membership resolves in one request, measured at 1.16s
against ~30s. The TUI's background poll inherits it through `sync_all` with no
per-surface wiring.

One request is one failure domain for transport. A timeout or a rate limit now
fails the whole round where a late request used to fail alone and leave earlier
work banked. The round is read-only and every cache is written after parsing, so
a failed round leaves every prior cache intact -- the same posture as a per-type
failure, at round granularity. Per-subtree failures are covered separately by
ADR-025.

Rate limiting shifts from request count to point cost: GraphQL charges by
computed node count, so one big query costs more points than one small one.
Total points per fetch stay comparable while requests drop ~50x, and the
secondary limit on serial requests is relieved outright.

A large document is harder to read than a small one. It is generated from config
rather than hand-written, and every field selection is a named constant beside
the struct that parses it, so the document and its parser cannot drift apart.

Test doubles answer a composed round rather than a narrow method. The parser is
pure and fixture-driven, so most tests assert on `FetchSnapshot` construction
instead of on a fake transport.

