---
title: Rendering integration
type: iteration
status: accepted
author: agent
date: 2026-03-11
tags: []
related:
- implements: STORY-057
---




## Changes

### Task Breakdown

1. **Add expand_refs function to engine module**
   - Create `src/engine/expand_refs.rs` module
   - Define function: `fn expand_refs(body: &str, repo_root: &Path) -> String`
   - Use regex to parse `@ref` directives: `@ref <path>#<symbol>[@<sha>]`
   - For each ref: resolve file content (from filesystem or git), extract symbol using tree-sitter, render as fenced code block
   - Handle language detection from file extension (`.ts` -> `ts`, `.rs` -> `rust`, `.py` -> `python`)
   - File: `src/engine/expand_refs.rs`

2. **Wire expand_refs into Store::get_body()**
   - Modify `Store::get_body()` to call `expand_refs()` after reading file content
   - Pass repo root (project root) to expand_refs for file resolution
   - File: `src/engine/store.rs`

3. **Export expand_refs from engine module**
   - Add `pub mod expand_refs;` to `src/engine/mod.rs`

4. **Handle error cases in expand_refs**
   - File not found: render warning block `> ⚠️ [unresolved: <path>#<symbol>]`
   - Symbol not found: render warning block with message about symbol not found
   - Invalid git SHA: render warning block with message about invalid SHA
   - Git unavailable: fall back to filesystem, render warning if file unavailable
   - File: `src/engine/expand_refs.rs`

5. **Test AC-1: CLI show replaces ref with code block**
   - Create test fixture with `@ref src/types/user.ts#UserProfile`
   - Run `lazyspec show <doc>` and verify output contains fenced code block with TypeScript
   - File: `tests/expand_refs_test.rs`

6. **Test AC-2: TUI preview replaces ref with code block**
   - Use existing TUI test infrastructure or create integration test
   - Verify preview tab renders ref as fenced code block
   - File: `tests/tui_expand_refs_test.rs`

7. **Test AC-3: Valid git SHA support**
   - Create test with `@ref src/types/user.ts#UserProfile@<valid_sha>`
   - Verify code block content matches file at that commit
   - File: `tests/expand_refs_test.rs`

8. **Test AC-4: Non-existent file warning**
   - Create test with `@ref nonexistent.ts#Foo`
   - Verify output contains `> ⚠️ [unresolved: nonexistent.ts#Foo]`
   - File: `tests/expand_refs_test.rs`

9. **Test AC-5: Non-existent symbol warning**
   - Create test with `@ref src/types/user.ts#NonExistent`
   - Verify output contains warning about symbol not found
   - File: `tests/expand_refs_test.rs`

10. **Test AC-6: Invalid SHA warning**
    - Create test with `@ref src/types/user.ts#UserProfile@invalid_sha`
    - Verify output contains warning about invalid SHA
    - File: `tests/expand_refs_test.rs`

11. **Test AC-7: TypeScript language tag**
    - Verify code fence uses `ts` for `.ts` files
    - File: `tests/expand_refs_test.rs`

12. **Test AC-8: Rust language tag**
    - Verify code fence uses `rust` for `.rs` files
    - File: `tests/expand_refs_test.rs`

13. **Test AC-9: Mixed resolved and unresolved refs**
    - Create document with both valid and invalid refs
    - Verify valid refs show as code blocks, invalid as warnings
    - Verify rest of document renders normally
    - File: `tests/expand_refs_test.rs`

## Test Plan

### AC Coverage

| AC | Description | Test Name |
|----|--------------|-----------|
| AC-1 | CLI show replaces ref with code block | `test_cli_show_expands_ref` |
| AC-2 | TUI preview replaces ref with code block | `test_tui_preview_expands_ref` |
| AC-3 | Valid git SHA support | `test_git_sha_ref` |
| AC-4 | Non-existent file warning | `test_nonexistent_file_warning` |
| AC-5 | Non-existent symbol warning | `test_nonexistent_symbol_warning` |
| AC-6 | Invalid SHA warning | `test_invalid_sha_warning` |
| AC-7 | TypeScript language tag | `test_typescript_language_tag` |
| AC-8 | Rust language tag | `test_rust_language_tag` |
| AC-9 | Mixed resolved/unresolved refs | `test_mixed_refs` |

### Test Cases

1. **test_cli_show_expands_ref**: Given document with `@ref src/types/user.ts#UserProfile`, when running `lazyspec show`, then output contains fenced code block with TypeScript definition
2. **test_tui_preview_expands_ref**: Given document with ref, when viewing in TUI preview, then ref shows as code block
3. **test_git_sha_ref**: Given `@ref src/types/user.ts#UserProfile@<sha>`, when rendered, then content from that commit is shown
4. **test_nonexistent_file_warning**: Given `@ref nonexistent.ts#Foo`, when rendered, then warning block shows with file not found message
5. **test_nonexistent_symbol_warning**: Given `@ref src/types/user.ts#NonExistent`, when rendered, then warning block shows with symbol not found message
6. **test_invalid_sha_warning**: Given `@ref src/types/user.ts#UserProfile@invalid`, when rendered, then warning block shows with invalid SHA message
7. **test_typescript_language_tag**: Given ref to `.ts` file, verify code fence uses `ts` language
8. **test_rust_language_tag**: Given ref to `.rs` file, verify code fence uses `rust` language
9. **test_mixed_refs**: Given document with both valid and invalid refs, verify mixed output

## Notes

- Shipped in commit 9d2a03b
- expand_refs module (`src/engine/expand_refs.rs`) was never created; logic went into `store.rs`
- TUI test (`tests/tui_expand_refs_test.rs`) was never created
- Warning format uses HTML comments instead of blockquote format from spec
- Expansion is synchronous and runs on every TUI render frame (performance issue)
- Debug eprintln statements left in test_mixed_resolved_and_unresolved_refs
