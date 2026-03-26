---
title: Ref model and blob hash parsing
type: iteration
status: accepted
author: agent
date: 2026-03-26
tags: []
related:
- implements: docs/stories/STORY-085-blob-pinning-with-semantic-hashing.md
---



## Changes

### Task 1: Introduce `Ref` struct to model parsed ref directives

**ACs addressed:** parse-symbol-blob-ref, parse-file-blob-ref, unpinned-ref-unchanged

**Files:**
- Modify: `src/engine/refs.rs`

**What to implement:**
Define a `Ref` struct with the following fields:
- `path: String` — the file path from the ref directive
- `symbol: Option<String>` — the optional `#symbol` fragment
- `blob_hash: Option<String>` — the optional `@{blob:<hex>}` pinning hash
- `commit_sha: Option<String>` — the legacy optional `@<hex>` commit SHA (preserved for backward compatibility)
- `span: (usize, usize)` — byte offsets of the full match in the source text

Add a `parse_refs(content: &str) -> Vec<Ref>` function that uses the updated regex to extract all ref directives from a document (skipping those inside fenced code blocks). Each parsed match populates the `Ref` struct fields: `path` is always set; `symbol` is `Some` only when `#fragment` is present; `blob_hash` is `Some` only when `@{blob:<hex>}` is present; `commit_sha` is `Some` only when the legacy `@<hex>` form is present. When neither pinning suffix appears, both `blob_hash` and `commit_sha` are `None`.

**How to verify:**
Run `cargo test` — all new unit tests from Task 2 must pass. Run existing tests `cargo test --lib refs` to confirm no regressions.

---

### Task 2: Update `REF_PATTERN` regex to capture `@{blob:hash}` syntax

**ACs addressed:** parse-symbol-blob-ref, parse-file-blob-ref, unpinned-ref-unchanged

**Files:**
- Modify: `src/engine/refs.rs`

**What to implement:**
Replace the current regex:
```
r"@ref\s+([^#@\s]+)(?:#([^@\s]+))?(?:@([a-fA-F0-9]+))?"
```
with a new regex that adds a capture group for the `@{blob:<hex>}` suffix while preserving the existing legacy `@<hex>` capture group. The new pattern should be:
```
r"@ref\s+([^#@\s]+)(?:#([^@\s]+))?(?:@\{blob:([a-fA-F0-9]+)\}|@([a-fA-F0-9]+))?"
```
Capture groups:
1. path (required)
2. symbol (optional)
3. blob hash from `@{blob:...}` (optional, new)
4. legacy commit SHA from `@...` (optional, existing — now group 4 instead of 3)

Update all call sites that reference capture group 3 (the old commit SHA) to reference group 4 instead. Specifically update `expand()` and `expand_cancellable()` in `src/engine/refs.rs` where `cap.get(3)` extracts the SHA — change to `cap.get(4)`. Add `cap.get(3)` for blob hash where needed.

**How to verify:**
Run `cargo test` — existing regex tests in `src/engine/refs.rs::tests` must still pass (the `test_ref_regex_parsing_with_sha` test will need its group index updated from 3 to 4). All expansion tests must remain green.

---

### Task 3: Wire `Ref` struct into `RefExpander` methods

**ACs addressed:** parse-symbol-blob-ref, parse-file-blob-ref, unpinned-ref-unchanged

**Files:**
- Modify: `src/engine/refs.rs`
- Modify: `src/engine/refs/resolve.rs`

**What to implement:**
Refactor `expand()` and `expand_cancellable()` to internally use `parse_refs()` (or the updated regex with the new group layout) instead of duplicating regex logic. The `resolve_ref()` signature in `resolve.rs` does not need to change yet — it still accepts the legacy `sha: Option<&str>` for resolution. The blob hash field is parsed and stored in the `Ref` struct but is not consumed during expansion in this iteration (it will be used by the drift detection pipeline in Iteration 2).

Ensure that when `blob_hash` is `Some`, the legacy `commit_sha` is `None` (the two suffixes are mutually exclusive by regex alternation). When neither suffix is present, both are `None`, and the ref is treated as unpinned.

**How to verify:**
Run `cargo test` — all existing expand tests must pass unchanged. Run `cargo test --test expand_refs_test` for integration tests.

## Test Plan

### Tests for AC: parse-symbol-blob-ref

1. **test_parse_symbol_blob_ref_basic** — Parse `@ref src/engine/refs.rs#RefExpander@{blob:a1b2c3d4}`. Assert `path == "src/engine/refs.rs"`, `symbol == Some("RefExpander")`, `blob_hash == Some("a1b2c3d4")`, `commit_sha == None`.

2. **test_parse_symbol_blob_ref_full_sha** — Parse `@ref src/foo.rs#MyStruct@{blob:abc123def456abc123def456abc123def456abcd}` (40-char hash). Assert all fields parse correctly with the full-length hash.

3. **test_parse_symbol_blob_ref_in_sentence** — Parse `"See @ref src/foo.rs#Bar@{blob:dead0000} for details"`. Assert the ref is found with correct path, symbol, and blob hash, and surrounding text is not consumed.

### Tests for AC: parse-file-blob-ref

4. **test_parse_file_blob_ref** — Parse `@ref config/schema.json@{blob:cafebabe}`. Assert `path == "config/schema.json"`, `symbol == None`, `blob_hash == Some("cafebabe")`.

5. **test_parse_file_blob_ref_no_symbol** — Parse `@ref Cargo.toml@{blob:1234abcd}`. Assert `symbol` is `None` and `blob_hash` is `Some("1234abcd")`.

### Tests for AC: unpinned-ref-unchanged

6. **test_parse_unpinned_ref_with_symbol** — Parse `@ref src/foo.rs#MyStruct`. Assert `path == "src/foo.rs"`, `symbol == Some("MyStruct")`, `blob_hash == None`, `commit_sha == None`.

7. **test_parse_unpinned_ref_path_only** — Parse `@ref src/foo.rs`. Assert `path == "src/foo.rs"`, `symbol == None`, `blob_hash == None`, `commit_sha == None`.

8. **test_parse_legacy_commit_sha_ref** — Parse `@ref src/foo.rs#Bar@abc1234`. Assert `commit_sha == Some("abc1234")` and `blob_hash == None`. This confirms backward compatibility with the old pinning syntax.

### Regression tests

9. **test_regex_does_not_match_blob_syntax_as_legacy** — Parse `@ref src/foo.rs@{blob:aabb}`. Assert `commit_sha` is `None` (the `{blob:...}` form must not be confused with the legacy `@hex` form).

10. **test_multiple_refs_mixed_pinning** — Parse a string containing three refs: one unpinned, one with legacy SHA, one with blob hash. Assert each is parsed independently with correct field values.

11. **test_blob_ref_inside_code_fence_is_skipped** — Wrap `@ref src/foo.rs@{blob:aabb}` inside a fenced code block. Assert `parse_refs()` returns an empty list (or the ref is excluded).

12. **test_expand_with_blob_ref_falls_through_to_head** — Call `expand()` on content containing `@ref Cargo.toml@{blob:1234}`. Since blob hash is not yet used for resolution, the expander should resolve against HEAD and produce a valid code fence output (no panic, no error).

## Notes

This iteration covers only parsing and modeling. The blob hash is captured but not yet used for drift detection or content verification. That logic belongs to Iteration 2 (semantic hashing pipeline) and Iteration 3 (pin command).

The `@{blob:hash}` and `@commitsha` suffixes are mutually exclusive by regex alternation. A ref directive cannot have both. If a user writes `@ref path#sym@abc@{blob:def}`, the regex will match `@abc` as legacy SHA and `@{blob:def}` will not be captured — this is acceptable and can be documented as unsupported.
