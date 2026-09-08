---
title: Compute a document's staleness band
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- implements: STORY-272
- blocks: ITERATION-423
---

## Objective

`engine::staleness::compute` returns band, driver, anchor, age and drift for one document, on demand.

## Satisfies

STORY-272 AC1, AC2, AC3, AC4, AC5 -- the logic, tested through `compute`. The `show --json` surface those ACs name lands in the next slice.

## Context

- Story + ACs: STORY-272
- Anchor, age, drift and the band-by-driver table: RFC-069 §Design "Computation". Types: RFC-069 §Interfaces. Serialized shape: §Design "Outputs".
- Config lands in the previous slice; this one reads it.
- Touch:
  - `src/engine/git_ref.rs:55` `renames` is the template for `diff_stat`: trait method, real impl, a free `parse_*` fn with its own test, and the mock in `test_support` (`:400`, `:516`, `:693`) with a `with_diff_stat` setter and a recorded call.
  - `src/engine/staleness.rs` (new), declared in `src/engine.rs`.
  - `src/engine/git_ref.rs:48` `read_commit_timestamp` supplies the anchor commit's time.
  - `src/engine/store.rs:57` `governs_root` is the directory the globs resolve against, so that is where the diff runs -- not the docs root, which differs on a docs-repo split.
- RFC-069 names `git diff --stat`. `--shortstat` yields the three numbers on one line and `--numstat` yields them per file; take whichever parses without a regex.
- AC4 and AC5 are the no-git paths: a `drift` type missing `governs` or missing `reviewed` calls neither `diff_stat` nor `read_commit_timestamp`. Assert that off the mock's recorded calls, not just off the returned value.

## Tasks

1. Add `diff_stat` to `GitRefOps` with a real implementation and a mock, mirroring `renames`.
2. Test-first: `drift` type with `reviewed` and `governs`, mock reporting counts, gives `stale`, driver `drift` and those counts (AC1); zero counts give `fresh` (AC2).
3. Test-first: an `age` type anchored 10, 100 and 200 days back against `90d`/`180d` gives `fresh`, `aging`, `stale` (AC3).
4. Test-first: a `drift` type with no `governs`, and one with no `reviewed`, each give driver `age`, a band off the thresholds, zero drift, and no `diff_stat` call (AC4).
5. Test-first: with `reviewed`, the anchor is that sha and age comes from its commit time; without, the anchor is the document's `date`, age is measured from it, and `read_commit_timestamp` is never called (AC5).
6. Implement `compute`. Serde on `Staleness` matches RFC-069 §Outputs exactly -- the next slice prints the object, it does not reshape it.

## Out of scope

- Every caller. `show`, `why`, `validate`, TUI, web.
- Caching, on `DocMeta` or anywhere else. Named non-goal in RFC-069.
- Rename-aware drift. `diff_stat` counts; `renames` already exists for STORY-271's purpose and is not wired in here.

## Principles/conventions

`cargo run --quiet -- convention`. DICTUM-004: no real git in tests, the mock supplies the counts and the timestamps.

## Verification

The mock records the range and the path list `diff_stat` was called with, so AC4 and AC5's "never called" assertions read off `calls` rather than off an absence in the result.
