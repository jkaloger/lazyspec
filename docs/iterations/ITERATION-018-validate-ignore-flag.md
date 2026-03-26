---
title: Validate-Ignore Flag
type: iteration
status: accepted
author: agent
date: 2026-03-06
tags: []
related:
- implements: STORY-030
---




## Changes

### Task 1: Add `validate_ignore` to the document model and parser

**ACs addressed:** AC1, AC6

**Files:**
- Modify: `src/engine/document.rs`

**What to implement:**

Add a `validate_ignore: bool` field to `DocMeta` (line 114) and a corresponding `#[serde(default, rename = "validate-ignore")]` field to `RawFrontmatter` (line 126). The serde `default` attribute handles AC6 (defaults to false when absent).

In `DocMeta::parse()` (line 181), pass the new field through from `raw` to the constructed `DocMeta`.

**How to verify:**
```
cargo test
```

### Task 2: Skip ignored documents in validation

**ACs addressed:** AC2, AC3, AC4

**Files:**
- Modify: `src/engine/validation.rs`

**What to implement:**

In `validate_full()` (line 59), add an early `continue` at the top of the first loop (line 62) when `meta.validate_ignore` is true. This skips all source-side checks for ignored documents (broken links, rejected/superseded parent, orphaned acceptance, unlinked iteration, unlinked ADR).

For target-side checks in the second loop (line 126), when iterating children (line 137-151), filter out children where `validate_ignore` is true before evaluating the `AllChildrenAccepted` and `UpwardOrphanedAcceptance` rules. Also filter ignored children from the `all_accepted` check so an ignored child doesn't count toward or against the "all children accepted" logic.

**How to verify:**
```
cargo test
```

### Task 3: Surface flag in JSON output

**ACs addressed:** AC5

**Files:**
- Modify: `src/cli/json.rs`

**What to implement:**

In `doc_to_json()` (line 4), add `"validate_ignore": doc.validate_ignore` to the `json!()` macro. Only include the field when it's `true` to avoid cluttering output for normal documents -- use a conditional merge or always include it (simpler, consistent with how other fields work).

**How to verify:**
```
cargo run -- status --json
cargo run -- show STORY-030 --json
```

### Task 4: Support flag in create templates and update command

**ACs addressed:** AC1 (create path)

**Files:**
- Modify: `src/cli/update.rs`

**What to implement:**

The update command already works via string key matching (line 13-17), so `validate-ignore` will work without code changes as long as the key exists in frontmatter. No changes needed to `update.rs`.

For templates in `src/cli/create.rs`, do NOT add `validate-ignore` to the default templates. The field should only appear when explicitly added to legacy documents. This keeps new documents clean.

This task is effectively a no-op for create/update since the existing update mechanism handles arbitrary keys. Verify it works end-to-end.

**How to verify:**
```
cargo run -- create rfc "Test" --author test
# manually add validate-ignore: true to the file
cargo run -- update <path> --status accepted
cargo run -- validate --json
```

## Test Plan

### Test 1: Ignored document with broken link produces no error (AC2)

Write a document with `validate-ignore: true` and a broken `implements` link. Validate and assert no `BrokenLink` error.

Properties: isolated, fast, behavioral, specific.

### Test 2: Ignored document skips upward orphaned acceptance (AC3)

Set up: draft RFC, accepted story with `validate-ignore: true` implementing the RFC. Validate and assert no `UpwardOrphanedAcceptance` warning for that story.

Properties: isolated, fast, behavioral, specific.

### Test 3: Non-ignored documents still report errors (AC4)

Set up: two documents with validation issues -- one ignored, one not. Validate and assert the non-ignored document's error is still present.

Properties: isolated, fast, behavioral, specific.

### Test 4: Default is false (AC6)

Parse a document without `validate-ignore` in its frontmatter. Assert `validate_ignore` is `false` on the resulting `DocMeta`.

Properties: isolated, fast, deterministic, specific.

### Test 5: Ignored documents excluded from all-children-accepted check (AC2, AC4)

Set up: draft RFC with two stories. Story-1 is accepted, Story-2 is accepted with `validate-ignore: true`. Without the ignore, we'd get `AllChildrenAccepted`. With the ignore, Story-2 should be excluded from the children set -- so only Story-1 counts, and since not "all" children (just one) is in play, the warning should reflect only non-ignored children.

Properties: isolated, fast, behavioral, structure-insensitive.

### Test 6: JSON output includes validate_ignore field (AC5)

Create a `DocMeta` with `validate_ignore: true`, call `doc_to_json()`, and assert the output contains `"validate_ignore": true`.

Properties: isolated, fast, specific.

## Notes

The update command's string-based key matching means `validate-ignore` works out of the box for existing documents. Users can add the flag manually or via `lazyspec update <path> --validate-ignore true` if we wire that up as a CLI arg -- but that's a separate concern and not in scope for this iteration.

Task 4 is essentially verification-only. The real work is in Tasks 1-3.
