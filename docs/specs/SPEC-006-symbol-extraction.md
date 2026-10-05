---
title: Symbol Extraction
type: spec
status: draft
author: jkaloger
date: 2026-03-25
governs:
- src/engine/symbols.rs
tags:
- engine
- tree-sitter
- symbols
related:
- implements: STORY-056
---

## Summary

Symbol extraction resolves `@ref` directives to concrete source code. Given a file path and symbol name, the system parses the file with tree-sitter, walks the concrete syntax tree, and returns the full text span of the matching definition. Two languages are supported: Rust and TypeScript.

## The SymbolExtractor Trait

@ref src/engine/symbols.rs#SymbolExtractor

The `SymbolExtractor` trait defines a single method, `extract(&self, source: &str, symbol: &str) -> Option<String>`. Callers pass the raw source text and a symbol name; the extractor returns the full source text of the matched node, or `None` if no match is found. The trait is public and object-safe, so new language extractors can be added without modifying existing code.

## CST Walk

@ref src/engine/symbols.rs#find_symbol_node

Both extractors delegate to `find_symbol_node`, a recursive function that walks the tree-sitter CST using a `TreeCursor`. Each extractor supplies a matcher: declaration node kinds, whether leading attributes and docs attach, and whether test blocks resolve. For a declaration node, it checks the `name` field first, then falls back to the `type` field (how `impl_item` is matched, since impl blocks expose their target type via `type`). On a match, the function returns the byte span of the node (extended per language, see below) as a `String`. The walk is depth-first: descend into the first child, iterate siblings, backtrack to the parent.

The walk runs in two passes. The declaration pass runs first; the test-block pass runs only if it finds nothing. Declarations therefore win over test blocks wherever each sits in the tree.

## TypeScript Extractor

@ref src/engine/symbols.rs#TypeScriptSymbolExtractor

`TypeScriptSymbolExtractor` uses the `tree-sitter-typescript` grammar. It matches the following node types:

- `type_alias` and `type_alias_declaration` -- covers `type Foo = ...` declarations
- `interface_declaration` -- covers `interface Foo { ... }`
- `class_declaration` -- covers `class Foo { ... }` including inheritance via `extends`
- `function_declaration` -- covers `function foo(...)` including `async function`
- `enum_declaration` -- covers `enum Foo { ... }` including string-valued enums

### Test blocks

When no declaration matches, the extractor resolves Jest/Vitest call blocks by title. A call matches when its callee is `it`, `test` or `describe`, or one of those with `.skip` or `.only`, and its first argument is a string or a template string without substitutions. The symbol is compared to the title after unescaping (`\n`, `\t`, `\"`, `\'`, `\\` and `` \` ``; any other backslash is kept). Template strings with `${...}` never match.

The result is the enclosing expression statement, including the trailing semicolon, not just the call. Calls nested in a `describe` callback resolve like top-level ones. The first match in document order wins.

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

### Leading decorations

A Rust range starts at the earliest outer doc comment (`///`, `/** */`) or attribute (`#[...]`) attached to the item. The walk goes backwards through preceding siblings and skips blank lines and plain comments (`//`, `/* */`) between decorations. It stops at the previous item, an inner attribute (`#![...]`) or an inner doc comment (`//!`, `/*! */`), since inner docs belong to the enclosing module. Plain comments before the earliest decoration, or directly before an undecorated item, are not part of the range.

## Name Resolution

The extractor returns the first match in document order within the winning pass. When a source file contains both a `struct_item` and an `impl_item` for the same name, the struct is returned because it appears earlier in the tree. There is no mechanism to request a specific occurrence or to return multiple matches.

## Parser Lifecycle

Each call to `extract` constructs a new `Parser`, sets the language, and parses the source from scratch. There is no parser reuse or incremental parsing across calls.

## Extension

New languages are added by implementing `SymbolExtractor` and registering the implementation in `RefExpander::extract_symbol()` for the relevant file extension. The trait's single-method design keeps the contract minimal.
