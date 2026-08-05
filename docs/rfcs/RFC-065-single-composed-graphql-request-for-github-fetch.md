---
title: Single composed GraphQL request for GitHub fetch
type: rfc
status: accepted
author: Jack Kaloger
date: 2026-08-06
tags: []
related:
- related-to: RFC-057
---<!-- intent: propose a design and the decisions it forces, before code -->

## Summary

A fetch costs one `gh` subprocess per read, and each subprocess costs ~0.58s of network round trip regardless of payload size. A 10-type project pays ~51 requests (~30s) to refresh caches that a single composed GraphQL document can return in 1.16s, measured against the live API. Replace the fetch read layer — REST `issue list`, REST `milestones`, and the three `nodes(ids:)` enrichment batches — with one GraphQL document per fetch round, built from config: each github-issues type becomes a field alias on `repository.issues`, and sub-issues, blocked-by edges, project items, milestones, org issue types, and authority-board field schemas all ride the same request as inline selections. `sync_all`'s per-type `TypeSync` dispatch (RFC-057) is unchanged; only where each syncer's data comes from moves.

## Motivation

Problems, priority order:

1. **Fetch latency is request count, not work.** Measured with a `gh` shim on `PATH` over a real fetch: 5 requests, 3.88s wall. `gh --version` is 0.03s, so binary startup is noise; a trivial `gh api graphql -f query='query{viewer{login}}'` is 0.58s across repeated runs. Cost is `requests × RTT`, and every request is serial. The TUI background poll runs the same `sync_all` (`event_loop.rs:397`), so the poll interval is floored by the same figure and the UI holds `GithubIssuesStore`'s mutex across all of it.

2. **The request count scales with types, not just issues.** For `T` github-issues types, `N` issues per type, `B` authority boards:

   | Requests | Site |
   |---|---|
   | 1 | `milestone_cache.rs:22` REST `milestone_list` |
   | T | `issue_cache.rs:720` REST `issue_list` |
   | T×N | `issue_cache.rs:731` REST `issue_view` — issue-type-only discovery |
   | T×⌈N/100⌉ | `fetch_subissue_parentage` |
   | T×⌈N/100⌉ | `list_blocked_by_batch` |
   | T×⌈N/100⌉ | `reconcile_project_fields_into_cache` |
   | T | `refresh_schema_snapshot` (`issue_cache.rs:682`) — org issue types |
   | T×B×(1–2) | `gh_schema.rs:265` `fetch_project_fields` |

   Two amplifiers are pure waste. `refresh_schema_snapshot` runs **per type** and re-fetches identical org-level issue types every time. `try_org_then_user` (`gh_schema.rs:227`) fires the org query, and on a user-owned repo — which `jkaloger/lazyspec` is — always pays a second request to discover what one `__typename` would have told it.

3. **`discover_issues`'s issue-type branch is a true N+1.** `issue_cache.rs:726-737` resolves numbers via GraphQL search, then issues one REST `issue_view` per number. A type classified by native issue type costs `N` requests to enumerate.

4. **Batching round trips is the wrong axis.** The prior commit took blocked-by and project fields from `O(N)` to `O(N/100)` via `nodes(ids:)`. That is a real improvement and it is also the ceiling of that approach: `nodes(ids:)` still needs the ids first, so it is structurally a second round trip after discovery. GraphQL exposes `subIssues`, `blockedBy`, and `projectItems` directly on `Issue`, so the ids never need to leave the server.

Without this, every fetch feature adds another ~0.6s to both surfaces and the TUI poll keeps getting slower as types are added.

## Goals

- One GraphQL request per fetch round covers every configured type's issues plus all enrichment, milestones, org issue types, and authority-board field schemas. Verified achievable: 10 types + milestones + issue types + board fields in one request, 1.16s, no errors.
- Request count becomes a function of pages, not of types, issues, boards, or enrichment steps. `⌈largest type's issue count / 100⌉` requests, not `T×N`.
- Per-piece best-effort survives. A missing `project` scope must still leave board-bound docs' last known status intact and warn, exactly as today (`issue_cache.rs:459-467`).
- The issue-type N+1 and the org/user double-call are eliminated as consequences of the composed selection, not as separate work.
- `sync_all`, `TypeSync`, `SyncContext`, and `SyncOutcome` keep their shapes. Both surfaces stay in lockstep by construction, per RFC-057.
- Fetch becomes complete: the `FETCH_LIMIT = 500` cap and its truncation warning go away.

## Non-goals

- No change to mutation paths. `issue_view` stays the read-back for `store_dispatch.rs:859,907,1000`; single-doc reads after a write are correctly one request.
- No comment fetching. `issue_comments` stays on-demand in `show` (`cli/show.rs:44`).
- No parallelism. The win is removing requests, not overlapping them; concurrent `gh` subprocesses would complicate the TUI's mutex posture for a fraction of the same benefit.
- No change to the cache-on-disk format, `IssueMap`, the lock, or the nested sub-issue layout.
- Not a persistent GraphQL client or connection pool. `gh api graphql` stays the transport (DICTUM: `gh` is the GitHub seam); this is about how many times it is invoked.
- ClickUp and git-ref syncers are untouched.

## Design

### Composed document, built from config

New module `src/engine/gh_fetch.rs`. It builds one query from the configured types and returns a parsed snapshot; it does not touch the cache.

```rust
/// Everything one fetch round needs from GitHub, resolved in one request.
pub struct FetchSnapshot {
    /// Per type name: the issues matching that type's discovery rule.
    pub issues: HashMap<String, Vec<GhIssue>>,
    /// Per issue node id, in server order.
    pub sub_issues: HashMap<String, Vec<String>>,
    /// Per issue number, the numbers blocking it.
    pub blocked_by: HashMap<u64, Vec<u64>>,
    /// Per issue node id, its board memberships and field cells.
    pub project_items: HashMap<String, Vec<ProjectItem>>,
    pub milestones: Vec<GhMilestone>,
    pub issue_types: Vec<IssueTypeId>,
    /// Per authority board number.
    pub board_fields: HashMap<u64, (Vec<ProjectFieldId>, Vec<OptionId>, Vec<IterationId>)>,
    /// Types whose issue list has more pages, with the cursor to resume from.
    pub next_pages: HashMap<String, String>,
    /// One per failed subtree, derived from `errors[].path`.
    pub warnings: Vec<RefreshWarning>,
}

pub fn fetch_round(
    gh: &dyn GhGraphql,
    repo: &str,
    types: &[&TypeDef],
    boards: &[u64],
    cursors: &HashMap<String, String>,
) -> Result<FetchSnapshot>;
```

Each type gets an alias derived from its index, its discovery rule mapped onto GraphQL arguments:

```graphql
t0: issues(first: 100, states: [OPEN, CLOSED], labels: ["lazyspec:story"], after: $c0) {
  pageInfo { hasNextPage endCursor }
  nodes {
    id number url title body state updatedAt createdAt
    author { login }
    issueType { name }
    milestone { number }
    labels(first: 20) { nodes { name } }
    assignees(first: 10) { nodes { login } }
    subIssues(first: 50) { nodes { id number } }
    blockedBy(first: 50) { nodes { number } }
    projectItems(first: 10) { nodes { id project { number } fieldValues(first: 25) { ... } } }
  }
}
```

`GhIssue`'s serde names are already GraphQL-shaped (`updatedAt`, `createdAt` at `gh.rs:57-60`) because `gh issue list --json` returns camelCase, so the mapping is near 1:1. The two divergences are connections: REST gives `labels: [{name}]` and `assignees: [{login}]`, GraphQL gives `{nodes: [...]}`. A parse helper unwraps `nodes` rather than changing `GhIssue`, so every existing consumer of `GhIssue` is untouched.

`issueType { name }` inline is what removes the N+1. A type classified by issue type is now discovered by listing and filtering `issue_type` client-side; `search_issue_numbers_by_type` and the `issue_view` loop both go away, and `ISSUE_TYPE_SEARCH_PAGE_SIZE` and `search_truncation_warning` with them. A tag+issue-type rule filters on both fields of the same result set instead of intersecting a REST list with a search.

### Owner subtree: issue types and board schemas, org/user resolved in one shot

```graphql
owner {
  __typename
  ... on Organization {
    issueTypes(first: 50) { nodes { id name } }
    b7: projectV2(number: 7) { fields(first: 50) { ... } }
  }
  ... on User {
    b7: projectV2(number: 7) { fields(first: 50) { ... } }
  }
}
```

Both inline fragments carry the same alias for the same field. Field merging permits it — both resolve `ProjectV2` — and it is verified against the live API: `owner.__typename` came back `User` and `b3.fields.nodes` resolved 13 fields in one request. `try_org_then_user` (`gh_schema.rs:219`) is deleted: the discriminator becomes a selected field rather than a failed request. Because the owner subtree is per-repo, not per-type, `refresh_schema_snapshot`'s per-type re-fetch collapses to once per fetch by construction.

Issue types being org-only is expressed by the fragment: a user-owned repo simply has no `issueTypes` key, no error and no wasted request. This retires the "expected failure on a user repo" caveat at `issue_cache.rs:299-302`.

### Per-piece best-effort via `errors[].path`

The single-request design would be unacceptable if one failed subtree sank the whole fetch. It does not. GitHub returns partial data with a typed error naming the failed path — verified by requesting a nonexistent board alongside a valid issue list:

```
data.repository.t0.nodes    -> 2 issues, intact
data.repository.owner.bad   -> null
errors[0]                   -> { type: "NOT_FOUND", path: ["repository","owner","bad"] }
```

`GhCli::graphql` (`gh.rs:1667-1683`) already handles this correctly: it parses stdout first and returns `Ok(json)` whenever `data` is present, regardless of `gh`'s non-zero exit. No seam change is needed. The `gh: …` summary goes to stderr; stdout stays clean JSON.

So the parser reads each subtree independently and maps `errors[].path` onto the same warnings the per-request paths emit today: a null `owner.b7` becomes "could not refresh field schema for board 7 (keeping prior…)", a null `projectItems` becomes the project-scope warning, and the board-status preservation at `issue_cache.rs:459-467` keeps working unchanged because it keys off absent data, not off a failed call.

### Node budget

GitHub caps a query at 500,000 possible nodes, and the cap is computed from `first:` arguments, not from what is returned. Measured exactly:

```
1 type,  subIssues(100) blockedBy(100) projectItems(50) fieldValues(50) -> OK
2 types, same selection -> "requests up to 550,200 possible nodes which exceeds the maximum limit of 500,000"
```

`projectItems(50) × fieldValues(50)` is 2,550 of the 2,750 nodes per issue. Capping to `subIssues(50) blockedBy(50) projectItems(10) fieldValues(25)` gives 360 per issue, verified OK at 10 types and rejected at 16. `gh_fetch` computes the budget from the selection constants and the type count and splits types across ⌈T/12⌉ requests when a project exceeds it, so the arithmetic is explicit rather than a hardcoded assumption that one request always fits.

The caps are lower than today's. An issue with more than 50 sub-issues or on more than 10 boards would be truncated, which no real project hits — but truncation must not be silent: each capped connection selects `pageInfo { hasNextPage }` and a `true` becomes a warning naming the issue and the connection. Truncation then surfaces rather than corrupting a cache quietly.

### Pagination: composed cursor rounds

GraphQL pages at 100. Round 1 requests the first 100 for every type at once. Types reporting `hasNextPage` have their `endCursor` fed into a second request that again composes every still-unfinished type. Requests total `max pages across types`, not the sum — a project with one 300-issue type and nine small ones costs 3 requests, not 12.

This makes fetch complete, so `FETCH_LIMIT = 500` (`issue_cache.rs:393`) and its "there may be more" warning are removed. A bounded fetch was an artifact of `gh issue list --limit`, not a requirement.

### What the syncers consume

`fetch_round` runs once in `sync_all` before the per-type dispatch, and the resulting `FetchSnapshot` is lent to the syncers alongside `SyncContext`. `GhIssueSync::sync` and `GhMilestoneSync::sync` keep their signatures' intent and their cache-writing logic; they read from the snapshot instead of calling clients. `IssueCache::fetch_all` keeps its parse, layout, lock, and diff logic and loses only its four read call sites.

This retires the scaffolding from the preceding commit, whose whole purpose was making those reads cheaper: `project_items_batch`, `list_blocked_by_batch`, `GH_NODES_BATCH_MAX`, the batch queries and their parsers, `PrefetchedProjectItems` (`sync.rs`), and `reconcile_target_node_id` (`store_dispatch.rs`). `reconcile_project_fields_for_meta` stays — it is the injection logic, not the read — and is fed from the snapshot.

## Interfaces

- `src/engine/gh_fetch.rs` @draft — `FetchSnapshot`, `fetch_round`, query builder, response parser, node-budget arithmetic.
- `GhGraphql` — unchanged. `gh_fetch` composes queries and calls `graphql()`; the transport seam and its partial-data handling stay as they are.
- `GhIssueReader::issue_list` — no longer called by fetch. Retained on the trait; `issue_view` retained and still used by mutation read-back.
- `GhMilestoneApi::milestone_list` — no longer called by fetch. `milestone_cache::fetch_milestones` takes milestones from the snapshot.
- `gh_schema::try_org_then_user`, `fetch_snapshot`, `fetch_project_fields` — deleted; `GhSchemaSnapshot` and its resolvers stay, populated from `FetchSnapshot`. The merge-not-overwrite posture at `issue_cache.rs:326-328` is preserved.
- `issue_cache::search_issue_numbers_by_type`, `ISSUE_TYPE_SEARCH_PAGE_SIZE`, `search_truncation_warning`, `FETCH_LIMIT` — deleted.
- `gh::GH_NODES_BATCH_MAX`, `project_items_batch`, `list_blocked_by_batch`, `sync::PrefetchedProjectItems`, `store_dispatch::reconcile_target_node_id` — deleted.
- `GhIssueDependencyApi::list_blocked_by` — retained for the mutation path; the fetch read-back comes from the snapshot.
- CLI and TUI: no signature changes. `lazyspec fetch --json` output shape is unchanged except that truncation warnings no longer occur.

## Decisions (ADRs to emit)

1. **One composed GraphQL document is the fetch read layer.** Request count, not payload, is the cost of a fetch; GraphQL's inline connections remove the second round trip that `nodes(ids:)` batching structurally requires.
2. **Nested connections are capped below GitHub's node budget, and truncation warns.** Fidelity is traded for request count at limits no real project reaches, and `hasNextPage` makes the trade observable rather than silent.
3. **Pagination composes across types per round.** Requests scale with the largest type's page count, not the sum, which is what lets the 500-issue cap be removed instead of raised.
4. **Partial GraphQL responses are the error model.** `errors[].path` maps to per-subtree warnings, preserving RFC-057's per-piece best-effort posture under a single request.
5. **Owner account kind is a selected field, not a failed request.** `__typename` with both inline fragments replaces the org-then-user retry.

## Stories

1. **`gh_fetch` query builder and parser.** `FetchSnapshot`, alias generation from type rules, node-budget arithmetic and type splitting, `errors[].path` → warnings, connection-truncation warnings. Pure, no I/O; tested against recorded fixtures. No call-site changes — lands dark.
2. **Milestones and owner subtree from the snapshot.** `milestone_cache::fetch_milestones` and `refresh_schema_snapshot` read the snapshot; delete `try_org_then_user`, `fetch_snapshot`, `fetch_project_fields`. Smallest real cut-over, and it alone removes the per-type issue-types re-fetch and the user-repo double call.
3. **Issue discovery and enrichment from the snapshot.** `discover_issues` and the three enrichment reads in `fetch_all` are replaced. Deletes the issue-type N+1 and the batch scaffolding from the preceding commit.
4. **Composed cursor pagination.** Multi-round `fetch_round`; remove `FETCH_LIMIT` and its warning.
5. **Both surfaces verified.** `lazyspec fetch --json` and the TUI poll traced with a `gh` shim asserting the request count; README updated for the removed truncation warning.

Sequence: 1 → 2 → 3 → 4 → 5. Stories 2 and 3 are independent of each other once 1 lands.

## Risks and tradeoffs

- **Truncation at the new caps is real.** An issue on more than 10 boards or with more than 50 sub-issues loses data where today it would not. Accepted because the caps exceed any observed usage and `hasNextPage` warns; ADR 2 records it. The escape hatch, if it is ever needed, is a per-issue follow-up query for overflowing connections only — deliberately not built now.
- **One query is one failure domain for transport.** Partial data covers per-subtree failures, but a transport error (timeout, rate limit, auth) now fails the whole round where previously a late request could fail alone and leave earlier work banked. Mitigation: the round is read-only and the cache is written after parsing, so a failed round leaves the previous cache intact — the same posture as today's per-type failure, at round granularity.
- **A large document is harder to read than a small one.** Mitigated by generating it from config rather than hand-writing it, and by keeping the field selections as named constants next to the structs they parse into.
- **Rate limiting shifts from request count to point cost.** GraphQL charges by computed node count, so one big query costs more points than one small one. Total points per fetch stay comparable while total requests drop by ~50×; the secondary rate limit on concurrent/serial requests is relieved outright.
- **Test doubles must grow.** Today's fakes implement narrow methods (`issue_list`, `project_items`); they will need to answer a composed query. Mitigation: story 1 makes the parser pure and fixture-driven, so most tests assert on `FetchSnapshot` construction rather than on a fake transport.
- **Removing `FETCH_LIMIT` makes a huge repo's first fetch slower in wall time** while being more correct. Bounded by pages, visible in the spinner, and the steady-state poll is unaffected.
