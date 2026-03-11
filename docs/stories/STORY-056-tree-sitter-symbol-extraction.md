---
title: Tree-sitter symbol extraction
type: story
status: accepted
author: agent
date: 2026-03-11
tags: []
related:
- implements: docs/rfcs/RFC-019-inline-type-references-with-ref.md
---



## Context

This story implements the symbol extraction layer for `@ref` directive expansion. The extraction uses tree-sitter to parse source files and extract type definitions by symbol name. This enables accurate extraction regardless of formatting, comments, or nesting.

## Acceptance Criteria

- **Given** a TypeScript source file containing a named type alias
  **When** `TypeScriptSymbolExtractor::extract()` is called with the source and type name
  **Then** it returns the full type definition including the type name

- **Given** a TypeScript source file containing an interface declaration
  **When** `TypeScriptSymbolExtractor::extract()` is called with the source and interface name
  **Then** it returns the full interface definition including all properties and methods

- **Given** a Rust source file containing a struct declaration
  **When** `RustSymbolExtractor::extract()` is called with the source and struct name
  **Then** it returns the full struct definition including all fields

- **Given** a Rust source file containing an enum declaration
  **When** `RustSymbolExtractor::extract()` is called with the source and enum name
  **Then** it returns the full enum definition including all variants

- **Given** a source file with a symbol that does not exist
  **When** the extractor is called with the non-existent symbol name
  **Then** it returns `None`

- **Given** a SymbolExtractor trait implementation
  **When** new language grammars need to be added
  **Then** they can implement the trait without modifying existing code

## Scope

### In Scope

- Add `tree-sitter` crate dependency to Cargo.toml
- Add `tree-sitter-typescript` crate dependency
- Add `tree-sitter-rust` crate dependency
- Define `SymbolExtractor` trait with `extract(source: &str, symbol: &str) -> Option<String>`
- Implement `TypeScriptSymbolExtractor` using tree-sitter-typescript grammar
- Implement `RustSymbolExtractor` using tree-sitter-rust grammar
- Extract named types, interfaces (TypeScript), types/aliases (TypeScript), structs (Rust), enums (Rust)
- Use tree-sitter queries to find symbol definitions by name
- Return full definition as string including the type name
- Make trait extensible for future languages

### Out of Scope

- CLI flag gating (`-e`/`--expand-references`)
- TUI lazy loading / async expansion
- Additional language grammars beyond TypeScript and Rust

## Implementation Notes

Shipped in commit 9d2a03b. The `SymbolExtractor` trait and both extractors live in `src/engine/symbol_extractor.rs`. Tests are inline in the module. Known issues: duplicated `visit_node` logic between extractors, potential double-advance sibling bug in tree cursor walk.
