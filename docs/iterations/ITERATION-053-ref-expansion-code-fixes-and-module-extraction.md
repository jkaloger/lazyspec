---
title: Ref expansion code fixes and module extraction
type: iteration
status: accepted
author: jkaloger
date: 2026-03-12
tags: []
related:
- implements: docs/stories/STORY-058-ref-expansion-hardening-and-performance.md
---



## Changes

### Task Breakdown

1. **Extract shared tree-walk helper in symbol_extractor.rs**
   - The `visit_node` inner function is duplicated between `TypeScriptSymbolExtractor::extract` and `RustSymbolExtractor::extract`. Both do the same cursor walk, differing only in which AST node types they match.
   - Create a top-level function: `fn find_symbol_node(cursor: &mut TreeCursor, source: &str, symbol: &str, match_node_types: &[&str]) -> Option<String>`
   - Both extractors call this with their respective node types: TS passes `["type_alias", "type_alias_declaration", "interface_declaration"]`, Rust passes `["struct_item", "enum_item"]`.
   - File: `src/engine/symbol_extractor.rs`

2. **Fix double-advance sibling bug in tree cursor walk**
   - In the current `visit_node` (lines 66-68 for TS, 140-142 for Rust), after the child loop calls `cursor.goto_parent()`, there's a stray `cursor.goto_next_sibling()` + recursive call. This can skip nodes because the parent's loop already handles sibling iteration.
   - Remove the stray `goto_next_sibling` block from `find_symbol_node` (which replaces `visit_node`).
   - Add a regression test: source with two type definitions where the first is nested inside a module/namespace, and the target symbol is the second top-level definition.
   - File: `src/engine/symbol_extractor.rs`

3. **Extract ref expansion into `src/engine/ref_expander.rs`**
   - Move `expand_refs`, `resolve_ref`, `extract_symbol`, and `language_from_extension` out of `Store`.
   - Create `RefExpander` struct holding a `PathBuf` (repo root). Methods: `pub fn expand(&self, content: &str) -> Result<String>`, `fn resolve_ref(...)`, `fn extract_symbol(...)`.
   - `Store::get_body` creates a `RefExpander` and delegates. Later iterations will use `RefExpander` independently.
   - Move `language_from_extension` to a free function in `ref_expander.rs`.
   - Export `pub mod ref_expander;` from `src/engine/mod.rs`.
   - File: `src/engine/ref_expander.rs`, `src/engine/store.rs`, `src/engine/mod.rs`

4. **Align test regex with production code**
   - The unit tests in `store.rs` (lines 665-708) use `([^#@]+)` but production uses `([^#@\s]+)`. Extract the regex pattern as a `pub const REF_PATTERN: &str` in `ref_expander.rs` and use it in both production and test code.
   - File: `src/engine/ref_expander.rs`, test sections in `src/engine/store.rs` (move regex tests to `ref_expander.rs`)

5. **Fix warning format to match spec**
   - Change `resolve_ref` error output from `<!-- @ref error: could not load {} -->` to `> [unresolved: {}]` to match STORY-057 AC.
   - When a symbol is specified but not found, fall back to the full file content (current behavior is correct here, just the file-not-found case needs fixing).
   - Update `tests/expand_refs_test.rs` assertions that check for `<!-- @ref error` to check for `> [unresolved:` instead.
   - File: `src/engine/ref_expander.rs`, `tests/expand_refs_test.rs`

6. **Remove debug eprintln from tests**
   - Delete the debug output lines (356-376) in `test_mixed_resolved_and_unresolved_refs` in `tests/expand_refs_test.rs`.
   - File: `tests/expand_refs_test.rs`

## Test Plan

| AC | Test | Validates |
|----|------|-----------|
| Symbol extraction correctness | `test_find_second_toplevel_type_after_nested` | Double-advance bug fix (regression) |
| Symbol extraction correctness | Existing `test_extract_*` tests still pass | Shared helper doesn't break extraction |
| Warning format | `test_ref_nonexistent_file_warning` (updated assertion) | Blockquote format |
| Warning format | `test_ref_invalid_sha_warning` (updated assertion) | Blockquote format |
| Module structure | `cargo test` full suite passes | RefExpander extraction doesn't break anything |
| Regex alignment | `test_ref_regex_*` tests use `REF_PATTERN` constant | Single source of truth for regex |

## Notes

- This iteration is purely refactoring and bug fixes. No behavior changes visible to end users except the warning format.
- The `RefExpander` extraction sets up iteration 2 (CLI flag) cleanly since `Store::get_body` can choose whether to use it.
