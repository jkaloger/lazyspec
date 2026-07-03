---
title: Dual materialization for overlapping type matches
type: story
status: complete
author: jkaloger
date: 2026-07-02
tags: []
related:
- implements: RFC-055
---## Problem

RFC-055 lets two `github-issues` types both match the same GitHub issue (e.g. both set `github_issue_type = "Feature"`, or one via `github_issue_tag` and the other via type). Once STORY-192 (per-type match-rule plumbing) and STORY-193 (issue-type GraphQL discovery) let each type's own fetch independently decide an issue satisfies its rule, nothing in `fetch`'s write path currently assumes — or guards against — the same issue being written into two different types' caches. This story confirms that write path already does the right thing with no changes, and locks that behavior in with a test.

## Goal

Prove `fetch` produces two independent cache docs when two types both match one issue, and that both docs individually refresh correctly afterward. Per RFC-055's Design ("Dual materialization" subsection), this requires **no new storage concept** — it is an emergent property of `fetch_all`/`refresh_stale` already being invoked once per type, into that type's own cache directory, with its own id prefix.

## Design

### Why this already works

`IssueCache::fetch_all` (src/engine/issue_cache.rs:302-448) and `IssueCache::refresh_stale` (src/engine/issue_cache.rs:131-245) are both called once per lazyspec type — see the per-type `label`/`labels` filter built at the top of each (issue_cache.rs:313-317 for `fetch_all`, issue_cache.rs:161-179 for `refresh_stale`) and passed to `gh.issue_list`. Each call:

- Resolves the doc id from that type's own prefix: `type_def.make_id(issue.number)` (issue_cache.rs:360, :209), which is `format!("{}-{}", self.prefix, suffix)` (src/engine/config.rs:989-991). Two types with different `prefix` values produce two different ids from the same `issue.number` (e.g. issue #42 → `STORY-42` for one type, `TICKET-42` for another).
- Writes into that type's own cache directory: `root.join(".lazyspec/cache").join(&type_def.name)` (issue_cache.rs:332; mirrored in `write_cache_file`/`write_cache_parent`/`write_cache_child`, src/engine/store_dispatch.rs:1649-1790, which join `.lazyspec/cache/<type_def.name>` before writing).
- Reads and writes its own `cache.lock` entries and `IssueMap` rows keyed by its own id (issue_cache.rs:403-424, :198-233).

None of this reads or touches any other type's cache directory, lock entries, or issue-map rows. So once STORY-192/193 land and two types' independent discovery queries both surface the same issue number, calling `fetch_all` (or `refresh_stale`) for type A and then for type B — the same two calls that already run today, one per configured type — naturally writes two docs, one per type's cache directory, each with its own id. Dual materialization is fetch no longer assuming each issue belongs to exactly one type's cache, per RFC-055's Design section; it is not a code change in this story.

### What this story actually adds

A round-trip test in `src/engine/issue_cache.rs`'s test module, alongside the existing `fetch_all`/`refresh_stale` tests (e.g. `test_fetch_all_populates_cache_with_frontmatter`, issue_cache.rs:1126, and the two-fetch `test_fetch_all_cleans_up_removed_issues`, issue_cache.rs:1246, are the closest existing shape to follow):

- Two `TypeDef`s with distinct `prefix`/`name`/`dir` (e.g. a `story_type_def()`-style helper and a second `ticket_type_def()`-style helper), both store `GithubIssues`.
- A `MockReader` returning an issue with the same `number` (e.g. `42`) for both types' `issue_list` calls — this simulates "both types' discovery independently matched issue #42" without needing STORY-192/193's real match-rule evaluation, since `MockReader::issue_list` already ignores its `_labels` argument and returns a fixed issue set (see issue_cache.rs:832-845).
- Call `cache.fetch_all(...)` once for each `TypeDef` against the same temp root and `IssueMap`.
- Assert both cache files exist under their own type directories (`.lazyspec/cache/<type-a>/<PREFIX-A>-42.md` and `.lazyspec/cache/<type-b>/<PREFIX-B>-42.md`), both parse with correct frontmatter (`type:` matching their own type name), and both `IssueMap` entries resolve to `issue_number: 42` under their own doc id.
- Call `cache.refresh_stale(...)` for each type after backdating both docs' lock entries, and assert both refresh independently (mirroring `test_refresh_stale_fetches_all_via_issue_list`, issue_cache.rs:941) — refreshing type A's doc does not touch type B's cache directory or lock entry, and vice versa.

No production code in `issue_cache.rs` or `store_dispatch.rs` is expected to change for this story; if the test above fails, that is itself the finding (see Risks below), not an assumption to route around silently.

### Accepted risk: last-write-wins across the two docs

Both materialized docs remain independently `update`-able. `GithubIssuesStore::update` (src/engine/store_dispatch.rs:952) reads the issue's *current* remote body fresh via `check_lock` (store_dispatch.rs:953), deserializes it through `issue_body::deserialize` under its own type's `IssueContext` (store_dispatch.rs:955-969), mutates, and writes back through `issue_body::serialize`'s single `<!-- lazyspec ... -->` comment block (src/engine/issue_body.rs:24,32). There is exactly one such comment block per GitHub issue. When type A's doc and type B's doc both point at issue #42, an `update` on one reads and rewrites that one shared block with no awareness the other type's doc also targets it — whichever `update` runs last wins, and the other doc's most recent locally-known state can silently diverge from what's now on GitHub until its own next `fetch`/`refresh`.

This is a **stated, accepted risk**, not a defect for this story to fix. RFC-055's Non-goals section is explicit: "No conflict detection or optimistic locking on dual-materialized writes... This matches RFC-050's existing house policy (\"last-write-wins + refresh; optimistic locking for native fields is out of scope\") — not a new risk class introduced here." This story must not introduce any ownership, precedence, or locking mechanism to arbitrate between the two docs — that is explicitly out of scope per RFC-055's Non-goals ("No precedence or ownership mechanism across overlapping type matches").

## Non-goals

- Per-type match-rule evaluation and discovery that decides *which* issues match which type's rule — STORY-192, STORY-193. This story assumes that decision has already been made (or, for its test, mocks the outcome directly).
- Any conflict detection, optimistic locking, ownership, or precedence mechanism across the two materialized docs. Explicitly out of scope per RFC-055's Non-goals; last-write-wins on the shared `<!-- lazyspec ... -->` comment block is accepted, matching RFC-050 precedent.
- Any change to `fetch_all`, `refresh_stale`, `write_cache_file`/`write_cache_parent`/`write_cache_child`, or `update`. This story is a test confirming existing per-type-scoped behavior already supports dual materialization, not a refactor.
- A `validate`/`fetch` diagnostic surfacing detected overlaps — flagged in RFC-055's Risks as a possible follow-up, not required here.
- `create` pushing native issue type — STORY-195. README documentation — STORY-196.

## Acceptance criteria

- **Given** two configured types with different `prefix` values (e.g. `story_type_def()` and a second type def), **when** each type's `fetch_all` is called against a `MockReader` that returns the same underlying issue number (e.g. `42`) for both, **then** two independent cache files exist afterward — one under each type's own `.lazyspec/cache/<type>/` directory, each named from its own type's prefix and the shared issue number (e.g. `STORY-42.md` and `TICKET-42.md`).
- **Given** those two materialized docs, **when** their frontmatter is parsed, **then** each correctly attributes `type:` to its own lazyspec type name (not the other's), and each type's `IssueMap` entry resolves the shared issue number under its own doc id.
- **Given** both docs' cache-lock entries backdated stale, **when** `refresh_stale` is called for type A and then for type B, **then** both refresh independently to fresh content, and refreshing one does not modify the other type's cache directory, cache-lock entry, or issue-map row.
- **Given** the above, **when** `update` is called on either doc, **then** it writes through the store's existing single-issue update flow unmodified (no new write path is introduced), and no test in this story asserts the two docs stay in sync — divergence via last-write-wins is the expected, accepted outcome per RFC-055's Non-goals.
