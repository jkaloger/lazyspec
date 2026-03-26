---
title: Validation Module Extraction
type: iteration
status: accepted
author: agent
date: 2026-03-05
tags: []
related:
- implements: STORY-028
---






## Problem

`store.rs` handles data loading, querying, search, and validation. After ITERATION-013 removes dead validation code, validation still accounts for ~130 lines of logic plus the `ValidationIssue` enum, `ValidationResult` struct, and Display impl. This is a natural module boundary.

## Changes

### Task 1: Create `src/engine/validation.rs` and move types + logic

**ACs addressed:** AC-8 (validation lives in a separate `validation.rs` module)

**Files:**
- Create: `src/engine/validation.rs`
- Modify: `src/engine/mod.rs` (add `pub mod validation`)
- Modify: `src/engine/store.rs` (remove moved code, make `docs` and `reverse_links` `pub(crate)`, keep thin delegation method)

**What to implement:**

Create `src/engine/validation.rs` containing:
- `ValidationIssue` enum (currently at `store.rs:378-393`) with all 8 variants
- `ValidationResult` struct (currently at `store.rs:395-399`) with `errors` and `warnings` fields
- `Display` impl for `ValidationIssue` (currently at `store.rs:408-437`)
- A public function `pub fn validate_full(store: &super::store::Store) -> ValidationResult` containing the validation logic currently in `Store::validate_full()` (currently at `store.rs:192-327`)

In `store.rs`:
- Remove `ValidationIssue`, `ValidationResult`, `Display` impl, and the body of `validate_full()`
- Change `docs` field from private to `pub(crate)`
- Change `reverse_links` field from private to `pub(crate)`
- Keep a thin delegation method on `Store`:
  ```rust
  pub fn validate_full(&self) -> crate::engine::validation::ValidationResult {
      crate::engine::validation::validate_full(self)
  }
  ```
  This preserves the existing caller API so `cli/validate.rs` and `cli/status.rs` don't need changes.

In `engine/mod.rs`:
- Add `pub mod validation;`

**How to verify:**
```
cargo test
```

---

### Task 2: Update test imports

**ACs addressed:** AC-8

**Files:**
- Modify: `tests/cli_validate_test.rs`
- Modify: `tests/cli_expanded_validate_test.rs`

**What to implement:**

These two test files import `ValidationIssue` from `store`:
```rust
use lazyspec::engine::store::ValidationIssue;
```

Change to:
```rust
use lazyspec::engine::validation::ValidationIssue;
```

The old import path will no longer work since the type has moved. The `store.rs` delegation method returns `ValidationResult` but doesn't re-export the type.

Alternatively, if Task 1 adds a re-export in `store.rs` (`pub use crate::engine::validation::{ValidationIssue, ValidationResult};`), then test imports don't need changing. Choose whichever approach is cleaner -- but updating the imports to point at the canonical location is preferred.

**How to verify:**
```
cargo test cli_validate_test cli_expanded_validate_test
```

## Test Plan

Existing test suite is the verification. Pure move refactor. Every test must pass unchanged.

| Test suite | What it verifies |
|------------|-----------------|
| All existing tests | Behavior preserved after module extraction |
| `cli_validate_test` | Validation checks still work via `validate_full()` |
| `cli_expanded_validate_test` | Advanced validation (upward consistency, warnings) unchanged |

## Notes

- Depends on ITERATION-013 (dead `validate()` / `ValidationError` already removed).
- `pub(crate)` on `docs` and `reverse_links` is the simplest approach. These fields are already effectively public through the validation and query methods.
