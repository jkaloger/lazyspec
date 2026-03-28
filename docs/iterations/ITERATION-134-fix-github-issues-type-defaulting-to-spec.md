---
title: fix github-issues type defaulting to spec
type: iteration
status: accepted
author: agent
date: 2026-03-28
tags: []
related:
- implements: STORY-095
---



## Problem

`testgh` documents from the github-issues store appear under specs. Two bugs contribute:

1. `GhCli::issue_list` (`src/engine/gh.rs:212`) doesn't pass `--state all` to the `gh` CLI. The CLI defaults to open-only. Closed issues are never fetched, so the cache is never refreshed with correct data.

2. `extract_type_and_tags` (`src/engine/issue_body.rs:133`) hardcodes `"spec"` as the fallback type when no `lazyspec:` label matches. This is a latent bug (the label matching works when issues are actually fetched), but still wrong as a default.

The stale cache files were written by an earlier code version before the fallback path was fixed. Since closed issues never get re-fetched, they persist with `type: spec`.

## Changes

### Task 1: Add `--state all` to `GhCli::issue_list`

ACs addressed: Store dispatch routing (complete issue coverage)

Files:
- Modify: `src/engine/gh.rs` (`issue_list` method, ~line 212)

What to implement:
Add `"--state"` and `"all"` to the args vec in `GhCli::issue_list`, so both open and closed issues are returned.

How to verify:
- `cargo test` passes
- `cargo run -- fetch --type testgh` returns 2 fetched (the closed issues)
- Cache files in `.lazyspec/cache/testgh/` have `type: testgh`

### Task 2: Thread type_name default through extract_type_and_tags

ACs addressed: Store dispatch routing (type correctness as defensive default)

Files:
- Modify: `src/engine/issue_body.rs`
- Modify: `src/engine/issue_cache.rs` (update `IssueContext` construction in `parse_issue`)
- Modify: `src/engine/store_dispatch.rs` (update `IssueContext` construction)
- Modify: `src/tui/infra/event_loop.rs` (update `IssueContext` construction)

What to implement:
1. Add a `default_type: String` field to `IssueContext`
2. Pass it through `deserialize` to `extract_type_and_tags` as a new `default_type: &str` parameter
3. Replace hardcoded `DocType::new("spec")` on line 133 with `DocType::new(default_type)`
4. Update all call sites constructing `IssueContext` to populate `default_type` from the configured type name

How to verify:
- `cargo test` passes
- Updated tests confirm type defaults to the configured type, not "spec"

## Test Plan

Task 1:
- Add test: `issue_list_includes_state_all` using `MockGhClient` or verify via integration test that closed issues are returned

Task 2:
- Update `extract_type_and_tags_defaults_to_spec` test to verify it defaults to the provided `default_type`
- Update `custom_type_defaults_to_spec_when_not_in_known_types` to assert the configured default
- Existing round-trip and label-matching tests continue to pass unchanged

## Notes

Root cause traced via systematic debugging. The label matching in `extract_type_and_tags` works correctly when issues are fetched. The visible symptom (type: spec in cache) was caused by stale cache files that were never refreshed because closed issues were excluded from fetch.
