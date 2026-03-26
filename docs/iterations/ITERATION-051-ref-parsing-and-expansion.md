---
title: Ref parsing and expansion
type: iteration
status: accepted
author: agent
date: 2026-03-11
tags: []
related:
- implements: STORY-055
---



## Changes

### Task Breakdown

1. **Add `expand_refs()` function to `src/engine/store.rs`**
   - Parse `@ref` directives using regex: `@ref (path#symbol@SHA | path#symbol | path@SHA)`
   - Extract file path, symbol name, and optional git SHA from directive
   - Resolve file content via `git show <SHA or HEAD>:<path>`
   - Derive language tag from file extension (.ts -> ts, .rs -> rust, .py -> python)
   - Replace directive with fenced code block placeholder containing extracted content
   - Handle multiple directives in same document, preserving order

2. **Integrate `expand_refs()` into `Store::get_body()` at `src/engine/store.rs:250`**
   - After extracting body, call `expand_refs()` before returning
   - Pass git repo root path to enable `git show` commands

3. **Add unit tests for each AC**
   - Test file: `tests/ref_expansion_test.rs`
   - AC1: Single `@ref src/foo.rs#MyStruct` replaced with fenced code block
   - AC2: `@ref src/utils.ts#SomeInterface@abc1234` resolves against git commit abc1234
   - AC3: Multiple `@ref` directives all replaced in order
   - AC4: `.ts` extension -> language tag `ts`
   - AC5: `.rs` extension -> language tag `rust`
   - AC6: `.py` extension -> language tag `python`
   - AC7: Correct extraction of file path and symbol from directive

## Test Plan

1. **Unit tests for ref parsing** (`tests/ref_expansion_test.rs`)
   - `test_parse_ref_directive_basic`: Parse `@ref src/foo.rs#MyStruct` -> (path: "src/foo.rs", symbol: "MyStruct", sha: None)
   - `test_parse_ref_directive_with_sha`: Parse `@ref src/utils.ts#SomeInterface@abc1234` -> (path: "src/utils.ts", symbol: "SomeInterface", sha: Some("abc1234"))
   - `test_parse_multiple_refs`: Multiple directives parsed in order
   - `test_language_tag_derivation`: Verify .ts -> ts, .rs -> rust, .py -> python

2. **Integration tests for expansion**
   - `test_expand_ref_replaces_with_fenced_block`: Directive replaced with ```rust\n...\n```
   - `test_expand_ref_with_git_sha`: Uses `git show <sha>:<path>` for resolution
   - `test_expand_ref_with_head`: Uses `git show HEAD:<path>` when no SHA

3. **End-to-end test via CLI**
   - Create doc with `@ref` directive, run `lazyspec show --json`, verify body contains fenced code block

## Notes

- Shipped in commit 9d2a03b alongside ITERATION-050 and ITERATION-052
- Symbol extraction was integrated in the same commit (not delegated)
- expand_refs and resolve_ref were added as methods on Store rather than as a separate module
- Regex in unit tests differs from production regex (known issue)
