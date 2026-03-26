---
title: Tree-sitter symbol extraction
type: iteration
status: accepted
author: agent
date: 2026-03-11
tags: []
related:
- implements: STORY-056
---




## Changes

### Task Breakdown

1. **Add tree-sitter dependencies to Cargo.toml**
   - Add `tree-sitter = "0.24"` dependency
   - Add `tree-sitter-typescript = "0.23"` dependency
   - Add `tree-sitter-rust = "0.23"` dependency
   - File: `Cargo.toml`

2. **Create SymbolExtractor trait module**
   - Define `SymbolExtractor` trait with `extract(source: &str, symbol: &str) -> Option<String>` method
   - Create new module `src/engine/symbol_extractor.rs`
   - Export from `src/engine/mod.rs`

3. **Implement TypeScriptSymbolExtractor**
   - Create `TypeScriptSymbolExtractor` struct implementing `SymbolExtractor` trait
   - Use tree-sitter-typescript grammar to parse TypeScript source
   - Implement extraction for type aliases (using `type_alias` node)
   - Implement extraction for interfaces (using `interface_declaration` node)
   - Use tree-sitter queries to find symbol by name
   - File: `src/engine/symbol_extractor.rs`

4. **Implement RustSymbolExtractor**
   - Create `RustSymbolExtractor` struct implementing `SymbolExtractor` trait
   - Use tree-sitter-rust grammar to parse Rust source
   - Implement extraction for structs (using `struct_item` node)
   - Implement extraction for enums (using `enum_item` node)
   - Use tree-sitter queries to find symbol by name
   - File: `src/engine/symbol_extractor.rs`

5. **Write unit tests for all ACs**
   - Test: TypeScript type alias extraction (AC-1)
   - Test: TypeScript interface extraction (AC-2)
   - Test: Rust struct extraction (AC-3)
   - Test: Rust enum extraction (AC-4)
   - Test: Non-existent symbol returns None (AC-5)
   - Test: Trait is extensible - verify trait is public and has correct signature (AC-6)
   - File: `tests/symbol_extractor_test.rs`

## Test Plan

### AC Coverage

| AC | Description | Test Name |
|----|--------------|-----------|
| AC-1 | TypeScript type alias extraction | `test_typescript_type_alias` |
| AC-2 | TypeScript interface extraction | `test_typescript_interface` |
| AC-3 | Rust struct extraction | `test_rust_struct` |
| AC-4 | Rust enum extraction | `test_rust_enum` |
| AC-5 | Non-existent symbol returns None | `test_nonexistent_symbol` |
| AC-6 | Trait is extensible | `test_trait_extensibility` |

### Test Cases

1. **test_typescript_type_alias**: Given `type Foo = { bar: string };`, extract("source", "Foo") returns "type Foo = { bar: string };"
2. **test_typescript_interface**: Given interface with properties/methods, extract returns full interface definition
3. **test_rust_struct**: Given `struct User { name: String, age: u32 }`, extract returns full struct
4. **test_rust_enum**: Given enum with variants, extract returns full enum definition
5. **test_nonexistent_symbol**: Given non-existent symbol, extract returns None
6. **test_trait_extensibility**: Verify trait signature allows implementation without modifying existing code

## Notes

- Shipped in commit 9d2a03b
- Tests are inline in the module rather than in a separate test file as originally planned
- Tree cursor walk logic is duplicated between extractors (cleanup tracked separately)
- Grammars are re-initialized per call rather than cached
