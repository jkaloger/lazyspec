---
title: Store parse error collection
type: iteration
status: accepted
author: agent
date: 2026-03-08
tags: []
related:
- implements: docs/stories/STORY-044-store-parse-error-collection.md
---



## Changes

### Task 1: Add ParseError struct and collection to Store

**ACs addressed:** AC-1, AC-2

**Files:**
- Modify: `src/engine/store.rs`

**What to implement:**

Add a `ParseError` struct to the store module:

@ref src/engine/store.rs#ParseError@febc9353350b931358067b354a6fa96070e14c3d

Add a `parse_errors: Vec<ParseError>` field to the `Store` struct.

In `Store::load()`, replace all five `if let Ok(mut meta) = DocMeta::parse(&content)` sites (lines 45, 65, 89, 137) with `match` blocks that push to `parse_errors` on `Err`. The path stored should be the relative path (same as what would have been `meta.path`).

Also update `reload_file()` (line 294) to use `match` instead of `if let Ok`. On error, push to `self.parse_errors` (or remove a stale entry if the file previously failed and now succeeds).

Add a public accessor:

```rust
pub fn parse_errors(&self) -> &[ParseError] {
    &self.parse_errors
}
```

**How to verify:**
`cargo test` -- existing tests pass (valid docs still load). New tests in Task 3 verify error collection.

---

### Task 2: Surface parse errors in validate and status CLI output

**ACs addressed:** AC-3, AC-4

**Files:**
- Modify: `src/cli/validate.rs`
- Modify: `src/cli/status.rs`

**What to implement:**

In `src/cli/validate.rs`, update `run_json()` to include parse errors:

```rust
let parse_errors: Vec<_> = store.parse_errors().iter().map(|pe| {
    serde_json::json!({ "path": pe.path.display().to_string(), "error": pe.error })
}).collect();
```

Add `"parse_errors": parse_errors` to the JSON output object. Also update `run_human()` to print parse errors (using the existing `error_prefix()` style).

Update `run_full()` so the exit code is 2 if there are parse errors OR validation errors.

In `src/cli/status.rs`, update `run_json()` to include the same `parse_errors` array at the top level of the output (alongside `documents` and `validation`).

**How to verify:**
`cargo test` -- existing CLI tests pass. New tests in Task 3 verify parse error output.

---

### Task 3: Tests

**ACs addressed:** AC-1, AC-2, AC-3, AC-4

**Files:**
- Modify: `tests/store_test.rs`
- Modify: `tests/cli_validate_test.rs`
- Modify: `tests/cli_status_test.rs`

**What to implement:**

Add to `tests/store_test.rs`:

1. `store_collects_parse_errors` -- Write a doc with missing `status` field to `docs/rfcs/`. Load the store. Assert `store.parse_errors().len() == 1` and the error path matches the file. Assert `store.all_docs()` is empty (the broken doc didn't load).

2. `store_loads_valid_alongside_invalid` -- Write one valid RFC and one RFC missing the `date` field. Load the store. Assert `store.all_docs().len() == 1` (valid one loaded) and `store.parse_errors().len() == 1` (broken one tracked).

Add to `tests/cli_validate_test.rs`:

3. `validate_json_includes_parse_errors` -- Write a broken doc. Load store. Call `validate::run_json()`. Parse the JSON. Assert `parsed["parse_errors"]` is an array with one entry containing `"path"` and `"error"` keys.

Add to `tests/cli_status_test.rs`:

4. `status_json_includes_parse_errors` -- Write a broken doc alongside valid docs. Call `status::run_json()`. Parse the JSON. Assert `parsed["parse_errors"]` array is present and non-empty.

**How to verify:**
`cargo test`

## Test Plan

| Test | AC | Properties | Notes |
|------|-----|-----------|-------|
| `store_collects_parse_errors` | AC-1 | Isolated, Specific, Behavioral | Verifies errors are tracked, not silently dropped |
| `store_loads_valid_alongside_invalid` | AC-2 | Isolated, Predictive | Confirms valid docs unaffected by broken siblings |
| `validate_json_includes_parse_errors` | AC-3 | Isolated, Behavioral | Verifies JSON schema of parse error output |
| `status_json_includes_parse_errors` | AC-4 | Isolated, Behavioral | Verifies status command includes errors |

## Notes

The `reload_file()` method also needs updating since it's called when the TUI detects file changes. This keeps parse error state consistent after edits without a full reload.
