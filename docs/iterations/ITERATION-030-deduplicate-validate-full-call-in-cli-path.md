---
title: "Deduplicate validate_full call in CLI path"
type: iteration
status: draft
author: "agent"
date: 2026-03-07
tags: []
related: []
validate-ignore: true
---

## Changes

### Task 1: Pass ValidationResult into formatting functions

**Files:**
- Modify: `src/cli/validate.rs`

**What to implement:**

`run_full` calls `store.validate_full(config)` on line 18, then `run_json` and `run_human` each call it again internally. Call it once in `run_full` and pass the `ValidationResult` to the formatting functions.

Change `run_json` signature from `(store, config)` to `(result: &ValidationResult)`. Change `run_human` signature from `(store, config, show_warnings)` to `(result: &ValidationResult, show_warnings)`. Update `run_full` to pass the result it already has.

If `run_json` or `run_human` are called from elsewhere, update those call sites too (check `src/cli/status.rs`).

**How to verify:**
```
cargo test
```

## Test Plan

Existing tests cover validation output. No new tests needed -- this is a pure refactor with no behavioral change. `cargo test` passing is sufficient.

## Notes

`run_full` currently calls `validate_full` once for the exit code, then `run_json`/`run_human` each call it again. This means validation runs twice per CLI invocation. Straightforward fix: compute once, pass the result.
