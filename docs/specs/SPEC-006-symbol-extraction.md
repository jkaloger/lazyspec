---
title: "Symbol Extraction"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: [engine, tree-sitter, symbols]
related:
  - implements: "docs/architecture/ARCH-003-engine/symbol-extraction.md"
  - implements: "docs/stories/STORY-056-tree-sitter-symbol-extraction.md"
---

## Summary

Symbol extraction resolves `@ref` directives to concrete source code. Given a file path and symbol name, the system parses the file with tree-sitter, walks the concrete syntax tree, and returns the full text span of the matching definition. Two languages are supported: Rust and TypeScript.

## The SymbolExtractor Trait

@ref src/engine/symbols.rs#SymbolExtractor

The `SymbolExtractor` trait defines a single method, `extract(&self, source: &str, symbol: &str) -> Option<String>`. Callers pass the raw source text and a symbol name; the extractor returns the full source text of the matched node, or `None` if no match is found. The trait is public and object-safe, so new language extractors can be added without modifying existing code.

## CST Walk

@ref src/engine/symbols.rs#find_symbol_node

Both extractors delegate to `find_symbol_node`, a recursive function that walks the tree-sitter CST using a `TreeCursor`. It accepts a list of node type strings to match against. For each node whose `kind()` matches one of those types, it checks the `name` field first, then falls back to the `type` field (which is how `impl_item` nodes are matched, since impl blocks expose their target type via the `type` field rather than `name`). When a match is found, the function returns the full byte span of the node as a `String`. The walk is depth-first: it descends into the first child, then iterates siblings, and backtracks to the parent.

## TypeScript Extractor

@ref src/engine/symbols.rs#TypeScriptSymbolExtractor

`TypeScriptSymbolExtractor` uses the `tree-sitter-typescript` grammar. It matches the following node types:

- `type_alias` and `type_alias_declaration` -- covers `type Foo = ...` declarations
- `interface_declaration` -- covers `interface Foo { ... }`
- `class_declaration` -- covers `class Foo { ... }` including inheritance via `extends`
- `function_declaration` -- covers `function foo(...)` including `async function`
- `enum_declaration` -- covers `enum Foo { ... }` including string-valued enums

## Rust Extractor

@ref src/engine/symbols.rs#RustSymbolExtractor

`RustSymbolExtractor` uses the `tree-sitter-rust` grammar. It matches the following node types:

- `struct_item` -- named structs, tuple structs, and unit structs
- `enum_item` -- enums with unit, tuple, or struct variants
- `function_item` -- free functions (`fn` / `pub fn`)
- `trait_item` -- trait definitions
- `impl_item` -- inherent impl blocks and trait impl blocks (matched via the `type` field, not `name`)
- `type_item` -- type aliases (`type Foo = ...`)
- `const_item` -- constants (`const FOO: T = ...`)
- `static_item` -- statics (`static FOO: T = ...`)
- `macro_definition` -- `macro_rules!` definitions

## Name Resolution

The extractor returns the first matching node encountered during the depth-first walk. When a source file contains both a `struct_item` and an `impl_item` for the same name, the struct is returned because it appears earlier in the tree. There is no mechanism to request a specific occurrence or to return multiple matches.

## Parser Lifecycle

Each call to `extract` constructs a new `Parser`, sets the language, and parses the source from scratch. There is no parser reuse or incremental parsing across calls.

## Extension

New languages are added by implementing `SymbolExtractor` and registering the implementation in `RefExpander::extract_symbol()` for the relevant file extension. The trait's single-method design keeps the contract minimal.
