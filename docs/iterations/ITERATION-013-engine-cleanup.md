---
title: Engine Cleanup
type: iteration
status: accepted
author: agent
date: 2026-03-05
tags: []
related:
- implements: STORY-028
---







## Problem

Code review identified several structural issues in the engine and CLI layers that increase maintenance risk and make the codebase harder to extend:

1. Frontmatter splitting is implemented four separate times across the codebase
2. The old `validate()` / `ValidationError` system is dead code (only `validate_full()` is called from main)
3. `DocType`, `Status`, and `RelationType` are parsed from strings via manual match blocks in 5+ locations instead of `FromStr` impls
4. `nucleo` and `pulldown-cmark` are listed as dependencies but never imported

This iteration consolidates shared logic, removes dead code, and adds standard trait implementations.

## Changes

### Task 1: Extract shared frontmatter splitting into `document.rs`

**Files:**
- Modify: `src/engine/document.rs` (make `split_frontmatter` public)
- Modify: `src/cli/update.rs` (remove `split_frontmatter_raw`, use shared version)
- Modify: `src/cli/link.rs` (remove `split_frontmatter_raw`, use shared version)
- Modify: `src/tui/app.rs` (remove inline frontmatter parsing in `update_tags`, use shared version)

**What to implement:**

`split_frontmatter` in `document.rs:99` is currently a private module-level function. Change its visibility to `pub` so it can be used from other modules.

The function signature is `fn split_frontmatter(content: &str) -> Result<(String, String)>` — it returns (yaml_str, body_str). This is the same signature as the `split_frontmatter_raw` copies in `update.rs:24` and `link.rs:60`.

In `update.rs`:
- Remove the local `split_frontmatter_raw` function (lines 24-36)
- Replace the call at line 9 with `crate::engine::document::split_frontmatter`

In `link.rs`:
- Remove the local `split_frontmatter_raw` function (lines 60-72)
- Replace calls at lines 8 and 37 with `crate::engine::document::split_frontmatter`

In `app.rs`:
- The `update_tags` function (lines 7-28) has inline frontmatter splitting (lines 11-16)
- Replace lines 11-16 with a call to `crate::engine::document::split_frontmatter`
- The variable names change slightly: the function returns `(yaml_str, body)` — adjust the destructuring to match

**How to verify:**
```
cargo test
```
All existing tests should pass unchanged since the behavior is identical.

---

### Task 2: Remove dead `validate()` and `ValidationError`

**Files:**
- Modify: `src/engine/store.rs` (remove `validate()` method and `ValidationError` enum + Display impl)
- Modify: `src/cli/validate.rs` (remove `run()` function)
- Modify: `tests/cli_validate_test.rs` (migrate tests to use `validate_full()`)

**What to implement:**

The old validation system consists of:
- `Store::validate()` at `store.rs:192-230` — returns `Vec<ValidationError>`
- `ValidationError` enum at `store.rs:418-422` — 3 variants (BrokenLink, UnlinkedIteration, UnlinkedAdr)
- `Display for ValidationError` at `store.rs:455-468`
- `cli::validate::run()` at `validate.rs:16-34` — the only caller of `store.validate()`

`main.rs:106` calls `run_full()`, not `run()`. So `run()` and everything it depends on is unreachable from the binary.

Remove:
1. The `validate()` method from `Store` (lines 192-230)
2. The `ValidationError` enum and its `Display` impl (lines 418-468)
3. The `run()` function from `cli/validate.rs` (lines 16-34)

Migrate `tests/cli_validate_test.rs`: all 5 tests call `store.validate()` and match on `ValidationError` variants. Rewrite them to call `store.validate_full()` and check `result.errors` against `ValidationIssue` variants instead. The variant names are the same (`BrokenLink`, `UnlinkedIteration`, `UnlinkedAdr`) so the match patterns just change their enum prefix.

For example, `validate_catches_broken_link` becomes:
```rust
let result = store.validate_full();
assert!(!result.errors.is_empty());
```

And `validate_catches_unlinked_iteration` becomes:
```rust
let result = store.validate_full();
let has_unlinked = result.errors.iter().any(|e| matches!(e, ValidationIssue::UnlinkedIteration { .. }));
assert!(has_unlinked);
```

**How to verify:**
```
cargo test cli_validate_test
cargo test cli_expanded_validate_test
```

---

### Task 3: Add `FromStr` impls for `DocType`, `Status`, and `RelationType`

**Files:**
- Modify: `src/engine/document.rs` (add `FromStr` impls)
- Modify: `src/cli/create.rs` (use `DocType::from_str` instead of manual match)
- Modify: `src/cli/list.rs` (use `DocType::from_str` and `Status::from_str`)
- Modify: `src/cli/search.rs` (use `DocType::from_str`)
- Modify: `src/tui/app.rs` (use `DocType` display for reverse mapping in `submit_create_form`)

**What to implement:**

Add `impl std::str::FromStr for DocType`:
```rust
impl std::str::FromStr for DocType {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "rfc" => Ok(DocType::Rfc),
            "adr" => Ok(DocType::Adr),
            "story" => Ok(DocType::Story),
            "iteration" => Ok(DocType::Iteration),
            _ => Err(anyhow::anyhow!("unknown doc type: {}", s)),
        }
    }
}
```

Add `impl std::str::FromStr for Status`:
```rust
impl std::str::FromStr for Status {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "draft" => Ok(Status::Draft),
            "review" => Ok(Status::Review),
            "accepted" => Ok(Status::Accepted),
            "rejected" => Ok(Status::Rejected),
            "superseded" => Ok(Status::Superseded),
            _ => Err(anyhow::anyhow!("unknown status: {}", s)),
        }
    }
}
```

Add `impl std::str::FromStr for RelationType`:
```rust
impl std::str::FromStr for RelationType {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "implements" => Ok(RelationType::Implements),
            "supersedes" => Ok(RelationType::Supersedes),
            "blocks" => Ok(RelationType::Blocks),
            "related-to" => Ok(RelationType::RelatedTo),
            _ => Err(anyhow::anyhow!("unknown relation type: {}", s)),
        }
    }
}
```

Then replace manual match blocks:

In `cli/create.rs:17-23`: replace the match that maps doc_type string to directory with:
```rust
let doc_type_parsed: DocType = doc_type.parse()
    .map_err(|_| anyhow!("unknown doc type: {}", doc_type))?;
let dir = match doc_type_parsed {
    DocType::Rfc => &config.directories.rfcs,
    DocType::Adr => &config.directories.adrs,
    DocType::Story => &config.directories.stories,
    DocType::Iteration => &config.directories.iterations,
};
```
This still needs the match for directory mapping, but the string-to-enum parse is now centralized.

In `cli/list.rs:8-13`: replace the `doc_type.and_then(|t| match ...)` with `doc_type.and_then(|t| t.parse().ok())`.

In `cli/list.rs:15-22`: replace the `status.and_then(|s| match ...)` with `status.and_then(|s| s.parse().ok())`.

In `cli/search.rs:8-14`: replace the match block with `DocType::from_str(dt).ok()`.

In `tui/app.rs:399-404`: replace the match on `self.create_form.doc_type` that produces a `&str` with a `to_string().to_lowercase()` call on the DocType (the Display impl already formats them — just lowercase it).

In `tui/app.rs:509-517`: replace the match on relation type strings with `RelationType::from_str(prefix.trim())`.

**How to verify:**
```
cargo test
```

---

### Task 4: Remove unused dependencies

**Files:**
- Modify: `Cargo.toml`

**What to implement:**

Remove these two lines from `[dependencies]`:
```
nucleo = "0.5"
pulldown-cmark = "0.12"
```

Neither crate is imported anywhere in the source. `pulldown-cmark` may be a transitive dependency of `tui-markdown`, but transitive deps don't need to be listed explicitly.

**How to verify:**
```
cargo build
cargo test
```
Both must succeed. If `tui-markdown` actually requires `pulldown-cmark` as a direct dep (re-exports types), the build will fail and it should be kept. In that case, only remove `nucleo`.

---

### Task 5: Fix inaccurate `AllChildrenAccepted` display message

**Files:**
- Modify: `src/engine/store.rs` (Display impl for `AllChildrenAccepted`)

**What to implement:**

At `store.rs:492-494`, the display message says "parent is draft" but the condition also triggers when the parent has status `Review`. Change the message from:

```rust
write!(f, "all children accepted but parent is draft: {} ({} children)", parent.display(), children.len())
```

to:

```rust
write!(f, "all children accepted but parent not accepted: {} ({} children)", parent.display(), children.len())
```

Also update any tests that assert on the exact string "parent is draft" — check `tests/cli_expanded_validate_test.rs` for string matching on `"all children accepted"`. The existing test at line 215 matches on `contains("all children accepted")` which will still pass.

**How to verify:**
```
cargo test cli_expanded_validate_test
```

## Test Plan

| Test | What it verifies | Properties traded |
|------|-----------------|-------------------|
| All existing `cli_validate_test` tests (migrated) | Old validation checks still work via `validate_full()` | Fast, Isolated, Specific |
| All existing `cli_expanded_validate_test` tests | Advanced validation unchanged | Fast, Isolated, Specific |
| `cargo build` after dep removal | No broken imports from removing nucleo/pulldown-cmark | Fast, Predictive |
| All existing `cli_create_test` tests | Document creation still works with FromStr-based parsing | Fast, Isolated |
| All existing `cli_link_test` tests | Link/unlink still works with shared frontmatter splitting | Fast, Isolated |
| All existing `cli_mutate_test` tests | Update still works with shared frontmatter splitting | Fast, Isolated |
| All existing `tui_submit_form_test` tests | TUI form submission still works | Fast, Isolated |

No new tests are needed. This is a pure refactor — every change preserves existing behavior. The existing test suite is the verification. If any test breaks, the refactor introduced a regression.

## Notes

- Task ordering matters: Task 1 should go first (shared frontmatter used by other tasks implicitly). Task 2 and 3 are independent. Task 4 is independent. Task 5 is independent. So after Task 1, the rest can go in any order.
- The `validate::run()` removal in Task 2 also removes dead code from the public API (`cli/validate.rs`). This is fine since `main.rs` only calls `run_full()`.
- `FromStr` impls in Task 3 could also enable clap's native enum parsing in the future (via `#[arg(value_enum)]` or `ValueEnum` derive), but that's out of scope for this iteration.
