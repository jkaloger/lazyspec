---
title: "Symbol Extraction"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: [engine, tree-sitter, symbols]
related:
  - implements: "docs/stories/STORY-056-tree-sitter-symbol-extraction.md"
---

## Acceptance Criteria

### AC: ts-type-alias-extraction

Given a TypeScript source string containing a named type alias (e.g. `type MyType = string | number`)
When `TypeScriptSymbolExtractor::extract()` is called with the source and the type name
Then it returns a `Some(String)` containing the full type alias declaration text

### AC: ts-interface-extraction

Given a TypeScript source string containing an interface declaration
When `TypeScriptSymbolExtractor::extract()` is called with the source and the interface name
Then it returns a `Some(String)` containing the full interface definition including all properties

### AC: ts-class-extraction

Given a TypeScript source string containing a class declaration (with or without `extends`)
When `TypeScriptSymbolExtractor::extract()` is called with the source and the class name
Then it returns a `Some(String)` containing the full class definition

### AC: ts-function-extraction

Given a TypeScript source string containing a function declaration (sync or async)
When `TypeScriptSymbolExtractor::extract()` is called with the source and the function name
Then it returns a `Some(String)` containing the full function definition

### AC: ts-enum-extraction

Given a TypeScript source string containing an enum declaration
When `TypeScriptSymbolExtractor::extract()` is called with the source and the enum name
Then it returns a `Some(String)` containing the full enum definition including all members

### AC: rust-struct-extraction

Given a Rust source string containing a struct declaration (named, tuple, or unit)
When `RustSymbolExtractor::extract()` is called with the source and the struct name
Then it returns a `Some(String)` containing the full struct definition

### AC: rust-enum-extraction

Given a Rust source string containing an enum declaration
When `RustSymbolExtractor::extract()` is called with the source and the enum name
Then it returns a `Some(String)` containing the full enum definition including all variants

### AC: rust-function-extraction

Given a Rust source string containing a function item
When `RustSymbolExtractor::extract()` is called with the source and the function name
Then it returns a `Some(String)` containing the full function definition including body

### AC: rust-trait-extraction

Given a Rust source string containing a trait definition
When `RustSymbolExtractor::extract()` is called with the source and the trait name
Then it returns a `Some(String)` containing the full trait definition including method signatures

### AC: rust-impl-extraction

Given a Rust source string containing an impl block (inherent or trait impl)
When `RustSymbolExtractor::extract()` is called with the target type name
Then it returns a `Some(String)` containing the first matching impl block, resolved via the `type` field

### AC: rust-type-const-static-macro-extraction

Given a Rust source string containing a type alias, const item, static item, or macro_rules definition
When `RustSymbolExtractor::extract()` is called with the corresponding name
Then it returns a `Some(String)` containing the full declaration

### AC: nonexistent-symbol-returns-none

Given a source string that does not contain the requested symbol name
When either extractor's `extract()` is called with that name
Then it returns `None`

### AC: empty-source-returns-none

Given an empty source string
When either extractor's `extract()` is called with any symbol name
Then it returns `None`

### AC: trait-is-object-safe-and-extensible

Given the `SymbolExtractor` trait
When a new struct implements it
Then the struct can be used as `&dyn SymbolExtractor` and as a generic `E: SymbolExtractor` parameter without modifying existing extractors

### AC: sibling-after-nested-module

Given a TypeScript source string where a symbol appears after a `declare module` block
When `TypeScriptSymbolExtractor::extract()` is called with that symbol's name
Then it returns the symbol, confirming the CST walk does not skip siblings after descending into nested nodes
