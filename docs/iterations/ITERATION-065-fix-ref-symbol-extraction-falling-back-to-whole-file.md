---
title: Fix @ref symbol extraction falling back to whole file
type: iteration
status: accepted
author: agent
date: 2026-03-13
tags: []
related:
- implements: STORY-058
---




## Context

When `@ref path#symbol` references a function, trait, impl, type alias, const, or static in Rust (or a class, function, const, or enum in TypeScript), the symbol extractor returns `None` because it only matches `struct_item`/`enum_item` (Rust) or `type_alias`/`interface_declaration` (TypeScript). The fallback in `resolve_ref` then dumps the entire file content into the preview.

A secondary issue: line-number refs (`@ref path#42`) return everything from line 42 to EOF instead of a bounded range.

**ACs addressed from STORY-058:**
- "Given a TypeScript or Rust source file with multiple type definitions, When the symbol extractor is called, Then it finds the correct symbol without skipping any"
- "Given an unresolvable @ref directive, When expansion runs, Then the output is `> [unresolved: path#symbol]`"

## Changes

### Task 1: Expand Rust tree-sitter node types

**ACs addressed:** symbol extractor finds correct symbol

**Files:**
- Modify: `src/engine/symbols.rs`
- Test: `src/engine/symbols.rs` (inline tests)

**What to implement:**

In `RustSymbolExtractor::extract`, expand the `match_node_types` slice from `["struct_item", "enum_item"]` to include:
- `function_item` (free functions)
- `trait_item` (trait definitions)
- `impl_item` (impl blocks -- note: `find_symbol_node` matches by `name` field, and `impl_item` uses `type` field for the type name, so this needs the name extraction to also check `child_by_field_name("type")`)
- `type_item` (type aliases)
- `const_item` (constants)
- `static_item` (statics)
- `macro_definition` (macro_rules!)

For `impl_item`, `find_symbol_node` currently only checks `child_by_field_name("name")`. `impl_item` nodes use `type` as the field name for the implementing type. Add a fallback in `find_symbol_node`: if `child_by_field_name("name")` returns `None`, also try `child_by_field_name("type")` and check the text of that node.

**How to verify:**
```
cargo test --lib engine::symbols
```
Verify new tests pass for function, trait, type alias, const, and impl extraction.

### Task 2: Expand TypeScript tree-sitter node types

**ACs addressed:** symbol extractor finds correct symbol

**Files:**
- Modify: `src/engine/symbols.rs`
- Test: `src/engine/symbols.rs` (inline tests)

**What to implement:**

In `TypeScriptSymbolExtractor::extract`, expand the `match_node_types` slice from `["type_alias", "type_alias_declaration", "interface_declaration"]` to include:
- `class_declaration`
- `function_declaration`
- `enum_declaration`

For `const`/`let` exports (`lexical_declaration`), the symbol name lives in a nested `variable_declarator` node, not directly on the `name` field. This is more complex and can be deferred to a follow-up. Document this limitation in Notes.

**How to verify:**
```
cargo test --lib engine::symbols
```
Verify new tests pass for class, function, and enum extraction.

### Task 3: Replace whole-file fallback with unresolved marker

**ACs addressed:** unresolvable @ref outputs `> [unresolved: path#symbol]`

**Files:**
- Modify: `src/engine/refs.rs`
- Test: `src/engine/refs.rs` (inline tests)

**What to implement:**

In `resolve_ref` at line 174-176, replace:
```rust
self.extract_symbol(path, sym, &file_content)
    .unwrap_or_else(|| file_content.to_string())
```
with:
```rust
match self.extract_symbol(path, sym, &file_content) {
    Some(content) => content,
    None => return Ok(format!("> [unresolved: {}#{}]", path, sym)),
}
```

This matches the existing error pattern used when `git show` fails (line 161).

Also fix the line-number branch (line 173): `lines[line_num - 1..]` returns everything from that line to EOF. Change to take a single line: `lines[line_num - 1].to_string()`.

**How to verify:**
```
cargo test --lib engine::refs
```
Verify that a ref to an unknown symbol produces `> [unresolved: ...]` instead of a code block with the full file.

## Test Plan

| Test name | AC | What it verifies |
|---|---|---|
| `test_extract_rust_function` | symbol extraction | `fn foo() {}` is extracted by name |
| `test_extract_rust_trait` | symbol extraction | `trait MyTrait {}` is extracted by name |
| `test_extract_rust_type_alias` | symbol extraction | `type Alias = ...` is extracted by name |
| `test_extract_rust_const` | symbol extraction | `const FOO: ...` is extracted by name |
| `test_extract_rust_impl` | symbol extraction | `impl MyStruct {}` is extracted by type name |
| `test_extract_ts_class` | symbol extraction | `class Foo {}` is extracted by name |
| `test_extract_ts_function` | symbol extraction | `function foo() {}` is extracted by name |
| `test_extract_ts_enum` | symbol extraction | `enum Color {}` is extracted by name |
| `test_unknown_symbol_returns_unresolved` | unresolved marker | Ref to non-existent symbol produces `> [unresolved: ...]` not file content |
| `test_line_number_ref_single_line` | line-number fix | `#42` returns only line 42, not 42-EOF |

All tests are unit-level, deterministic, fast, and isolated. No tradeoffs to flag.

## Notes

- `lexical_declaration` extraction for TypeScript (const/let exports) is out of scope. The name is nested in a `variable_declarator` child, requiring different traversal logic. Worth a follow-up if users reference TS constants frequently.
- STORY-058 marks "Additional language grammars (Python, Go, etc.)" as out of scope, so we only expand within existing Rust and TypeScript extractors.
- The `macro_definition` node type uses `name` field in tree-sitter-rust, so `find_symbol_node` handles it without changes.
