---
title: Ignore and Unignore CLI Commands
type: iteration
status: draft
author: agent
date: 2026-03-06
tags: []
related:
- implements: docs/stories/STORY-030-validate-ignore-flag.md
---


## Changes

### Task 1: Add `Ignore` and `Unignore` commands to the CLI

**Files:**
- Modify: `src/cli/mod.rs`
- Create: `src/cli/ignore.rs`
- Modify: `src/main.rs`

**What to implement:**

Add two new variants to the `Commands` enum in `src/cli/mod.rs`:

```rust
/// Mark a document to skip validation
Ignore {
    /// Document path
    #[arg()]
    path: String,
},
/// Remove validation skip from a document
Unignore {
    /// Document path
    #[arg()]
    path: String,
},
```

Add `pub mod ignore;` to the module list in `src/cli/mod.rs`.

Create `src/cli/ignore.rs` with two functions following the `link.rs` pattern (parse YAML as `serde_yaml::Value`, manipulate, serialize back):

**`ignore(root, doc_path)`**: Read the file, split frontmatter, parse YAML as `serde_yaml::Value`, set `doc["validate-ignore"] = true`, serialize back, write file.

**`unignore(root, doc_path)`**: Read the file, split frontmatter, parse YAML as `serde_yaml::Value`, remove the `validate-ignore` key from the mapping (use `as_mapping_mut().remove()`), serialize back, write file. If the key doesn't exist, succeed silently (idempotent).

Wire both commands in `src/main.rs` following the existing pattern (e.g. the `Link`/`Unlink` arms). Print confirmation: `"Ignoring {path}"` / `"Unignoring {path}"`.

**How to verify:**
```
cargo run -- ignore docs/stories/STORY-030-validate-ignore-flag.md
cargo run -- show STORY-030 --json  # should show validate_ignore: true
cargo run -- unignore docs/stories/STORY-030-validate-ignore-flag.md
cargo run -- show STORY-030 --json  # should show validate_ignore: false
```

### Task 2: Update README

**Files:**
- Modify: `README.md`

**What to implement:**

Add `ignore` and `unignore` to the CLI Reference table in the README:

```
| `ignore <path>`                      | Mark a document to skip validation                                    |
| `unignore <path>`                    | Remove validation skip from a document                                |
```

**How to verify:**

Read the README and confirm the table is correct.

## Test Plan

### Test 1: Ignore adds validate-ignore field

Create a document without `validate-ignore`. Run `ignore::ignore()`. Re-parse the document and assert `validate_ignore` is `true`.

Properties: isolated, fast, behavioral, specific.

### Test 2: Unignore removes validate-ignore field

Create a document with `validate-ignore: true`. Run `ignore::unignore()`. Re-parse and assert `validate_ignore` is `false`.

Properties: isolated, fast, behavioral, specific.

### Test 3: Ignore is idempotent

Create a document, run `ignore()` twice. Re-parse and assert `validate_ignore` is `true` (no duplicate keys, no error).

Properties: isolated, fast, deterministic.

### Test 4: Unignore on document without field succeeds

Create a document without `validate-ignore`. Run `unignore()`. Assert no error and document is unchanged.

Properties: isolated, fast, behavioral.

### Test 5: Ignore + validate end-to-end

Create an iteration without a story link (triggers UnlinkedIteration error). Run `ignore()`. Run `validate_full()`. Assert no error for that document.

Properties: isolated, fast, predictive. Trades structure-insensitivity for predictiveness (end-to-end).

## Notes

Following the `link.rs` pattern for YAML manipulation (parse as `serde_yaml::Value`, modify, serialize). This avoids the `update.rs` limitation of only being able to replace existing keys.
