---
title: GitHub store always derives ID from prefix and issue number
type: iteration
status: accepted
author: agent
date: 2026-03-28
tags: []
related:
- implements: STORY-095
---



## Problem

GitHub-issues store documents do not display their type prefix. `lazyspec show 33` shows `Type: testgh` but the document ID is bare `33` instead of `TESTGH-33`. The cache file is written as `.lazyspec/cache/testgh/33.md` instead of `.lazyspec/cache/testgh/TESTGH-33.md`.

Root cause: `issue_cache.rs` calls `extract_doc_id()` which tries to parse a `PREFIX-XXX` pattern from the issue title/body, falling back to the bare issue number on failure:
```rust
let id = extract_doc_id(issue, &type_def.name)
    .unwrap_or_else(|| issue.number.to_string());
```

GitHub issues have a stable, unique number assigned by GitHub. The document ID should always be `{PREFIX}-{issue_number}` — there is no reason to parse an ID from the title since the issue number *is* the canonical suffix.

## Changes

### Task 1: Add `TypeDef::make_id` method

**Files:**
- Modify: `src/engine/config.rs`

**What to implement:**

Add a method to `TypeDef` that consolidates the scattered `format!("{}-{}", prefix, suffix)` pattern:

```rust
impl TypeDef {
    pub fn make_id(&self, suffix: impl std::fmt::Display) -> String {
        format!("{}-{}", self.prefix, suffix)
    }
}
```

This replaces the bare `format!` calls across the codebase with a single, discoverable method. Callers that need zero-padding (e.g. `{:03}`) format the suffix before passing it in.

**How to verify:**
```
cargo test --lib
```

### Task 2: Replace all `format!("{}-{}", prefix, ...)` call sites with `make_id`

**Files:**
- Modify: `src/cli/fix/conflicts.rs` (line 90, 94)
- Modify: `src/cli/fix/renumber.rs` (line 119)
- Modify: `src/cli/reservations.rs` (line 96)
- Modify: `src/engine/template.rs` (line 89)

**What to implement:**

Replace each `format!("{}-{}", prefix, ...)` or `format!("{}-{}", type_def.prefix, ...)` with `type_def.make_id(...)`. Some call sites use a local `prefix` variable extracted from `type_def.prefix` — these need minor refactoring to have `type_def` in scope, or the local var can call the method.

Specific replacements:
- `conflicts.rs:90`: `format!("{}-{}", type_def.prefix, sqid)` → `type_def.make_id(&sqid)`
- `conflicts.rs:94`: `format!("{}-{:03}", type_def.prefix, new_num)` → `type_def.make_id(format_args!("{:03}", new_num))`
- `renumber.rs:119`: `format!("{}-{}", prefix, sqid)` → needs `type_def` in scope or keep local if `type_def` isn't available
- `reservations.rs:96`: `format!("{}-{}", prefix, formatted_number)` → same consideration
- `template.rs:89`: `format!("{}-{}", prefix, id)` → same consideration

For call sites where only `prefix: &str` is in scope (not the full `TypeDef`), either thread `type_def` through or leave as-is — don't force awkward plumbing just to use the method. The method is for call sites that already have `type_def`.

**How to verify:**
```
cargo test
# All tests pass, grep confirms no remaining format!("{}-{}", .*prefix patterns at call sites that have type_def
```

### Task 3: Use `make_id` in issue_cache and remove `extract_doc_id`

**Files:**
- Modify: `src/engine/issue_cache.rs`

**What to implement:**

Replace the two `extract_doc_id` call sites (in `refresh_stale` at line ~199 and `fetch_all` at line ~289):

```rust
// Before:
let id = extract_doc_id(issue, &type_def.name)
    .unwrap_or_else(|| issue.number.to_string());

// After:
let id = type_def.make_id(issue.number);
```

Remove the `extract_doc_id` function (lines 371-374) since it becomes dead code.

**How to verify:**
```
cargo run -- show TESTGH-33 --json
# Should show id: "TESTGH-33" and path containing TESTGH-33.md
```

### Task 4: Remove `extract_doc_id_from_title` and its tests

**Files:**
- Modify: `src/engine/issue_body.rs`

**What to implement:**

`extract_doc_id_from_title` (line 171) has no remaining callers after Task 3 removes `extract_doc_id`. Remove the function and its tests (lines 545-570).

**How to verify:**
```
cargo test
# All tests pass, no dead code warnings
```

### Task 5: Update existing tests that rely on title-based ID extraction

**Files:**
- Modify: `src/engine/issue_cache.rs` (test module)

**What to implement:**

The existing tests in `issue_cache.rs` use issue titles like `"STORY-001 First story"` and expect cache files named `STORY-001.md`. After the change, the ID will be derived as `{prefix}-{issue_number}` instead, so:

- `make_gh_issue(10, "STORY-001 First story", ...)` → ID becomes `STORY-10` (from prefix `STORY` + number `10`)
- Tests expecting `STORY-001` filenames and IDs need to be updated to expect `STORY-10`

Update all test assertions to match the new ID derivation. Ensure the tests cover:
1. Normal case: issue number maps to `{PREFIX}-{number}`
2. Title containing a lazyspec ID is ignored (ID comes from number, not title)
3. Fetch, refresh, and cleanup all use the new ID format

**How to verify:**
```
cargo test issue_cache
# All tests pass
```

### Task 6: Add test for issue without PREFIX in title

**Files:**
- Modify: `src/engine/issue_cache.rs` (test module)

**What to implement:**

Add a test that creates a GitHub issue with a plain title (no PREFIX-XXX pattern, like the real-world `"test"` title on issue #33) and verifies the document gets the correct prefixed ID.

```rust
#[test]
fn test_fetch_all_derives_id_from_prefix_and_number() {
    // Issue with plain title (no STORY-XXX)
    let issue = make_gh_issue(33, "test", "hi", &["lazyspec:story"]);
    // After fetch, ID should be "STORY-33", not "33"
    // Cache file should be .lazyspec/cache/story/STORY-33.md
}
```

**How to verify:**
```
cargo test test_fetch_all_derives_id_from_prefix_and_number
```

## Test Plan

1. **Unit: `make_id` produces correct format** — `TypeDef` with prefix `"STORY"` calling `make_id(42)` returns `"STORY-42"`. Calling `make_id(format_args!("{:03}", 7))` returns `"STORY-007"`. (Fast, Specific, Deterministic)

2. **Unit: ID always has prefix** — Create a mock issue with a plain title (no PREFIX-XXX). Fetch it. Assert the resulting document ID is `{PREFIX}-{number}` and the cache file path matches. This is the primary regression test. (Specific, Behavioral, Deterministic)

3. **Unit: Title-embedded ID is ignored** — Create a mock issue with title `"STORY-999 Some title"` but issue number `10`. Assert the ID is `STORY-10` (from the number), not `STORY-999` (from the title). Verifies the old parsing path is truly gone. (Specific, Behavioral)

4. **Unit: Existing refresh/fetch tests pass with updated expectations** — The existing test suite for `refresh_stale` and `fetch_all` must pass after updating expected IDs from title-derived to number-derived. (Predictive, Structure-insensitive)

5. **Integration: `lazyspec show TESTGH-33` displays prefixed ID** — Run `cargo run -- show TESTGH-33 --json` against a real cached issue and verify the JSON output contains the prefixed ID and correct path. (Predictive, Inspiring)

## Notes

- The RFC-037 "Issue Number Mapping" section describes `ITERATION-042` mapping to `#87` — implying a lazyspec-assigned ID separate from the issue number. The current implementation skips that mapping layer and uses the issue number directly as the suffix. This is simpler and matches the user's intent: GitHub issue numbers are already unique per repo and stable.
- `extract_doc_id_from_title` may still be useful if we later support importing issues that already have lazyspec IDs in their titles. But YAGNI — remove it now, restore from git history if needed.
- `show 33` (bare issue number) will stop working after this change — users must use `show TESTGH-33`. This is consistent with how all other document types work (`show RFC-037`, not `show 037`).
- Task 2 should only replace call sites where `type_def` is already in scope. Don't thread `type_def` through functions that currently only receive `prefix: &str` — that's a larger refactor for another day.
