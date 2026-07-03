---
title: Dual materialization for overlapping type matches
type: iteration
status: complete
author: jkaloger
date: 2026-07-03
tags: []
related:
- implements: STORY-194
---

## Objective

Lock in, with a test, that `fetch_all`/`refresh_stale` already write independent per-type cache docs when two types both match one issue -- no production code change expected.

## Context

- Story: STORY-194 (assumes STORY-192/193 landed -- ITERATION-263, ITERATION-264 -- for real match-rule evaluation; this story's test mocks the multi-match outcome directly, doesn't need real classification)
- Design (why this already works, exact test shape, accepted last-write-wins risk): STORY-194 body verbatim -- don't re-derive.
- Touch: src/engine/issue_cache.rs test module only. Closest existing shape: `test_fetch_all_populates_cache_with_frontmatter` (issue_cache.rs:1126), `test_fetch_all_cleans_up_removed_issues` (:1246), `test_refresh_stale_fetches_all_via_issue_list` (:941).

## Satisfies

STORY-194 AC1-AC4 (all -- one test scenario exercised across fetch + refresh, not separable).

## Tasks

1. Two `TypeDef` fixtures, distinct `prefix`/`name`/`dir`, both `store = GithubIssues`.
2. `MockReader` returning the same issue `number` (e.g. `42`) for both types' `issue_list` calls (existing `MockReader::issue_list` already ignores `_labels`, issue_cache.rs:832-845).
3. Call `fetch_all` once per `TypeDef` against the same temp root + `IssueMap`. Assert two cache files exist under each type's own `.lazyspec/cache/<type>/` dir, correctly named/prefixed, each parses with `type:` matching its own type, each `IssueMap` entry resolves `issue_number: 42` under its own doc id.
4. Backdate both docs' lock entries; call `refresh_stale` for type A then type B. Assert both refresh independently -- refreshing A doesn't touch B's cache dir/lock entry/issue-map row, and vice versa.
5. If any assertion fails: that is the finding to report, not something to route around with a production change (per story's explicit no-change expectation).

## Out of scope

- Match-rule evaluation/discovery deciding which issues match which type -- STORY-192/193 (mocked directly here).
- Any conflict detection, optimistic locking, ownership, or precedence mechanism -- explicitly out of scope per RFC-055 Non-goals; last-write-wins accepted (RFC-050 precedent).
- Any change to `fetch_all`, `refresh_stale`, `write_cache_file`/`write_cache_parent`/`write_cache_child`, or `update`.
- A `validate`/`fetch` overlap diagnostic -- flagged as a possible follow-up, not required here.
- `create` native issue type push -- STORY-195. README -- STORY-196.

## Principles/conventions

Test-only addition to existing issue_cache.rs test module (CONVENTION.md L4 -- real implementations by default, fakes only at trait seams; `MockReader` is the existing seam, no new fake introduced).

## Verification

- Two independent cache files exist after the two `fetch_all` calls, correctly attributed and indexed.
- Independent `refresh_stale` per type: no cross-contamination of cache dir, lock entry, or issue-map row.
- `update` on either doc still goes through the existing single-issue update flow unmodified; no assertion that the two docs stay in sync post-update (divergence via last-write-wins is the expected outcome).

