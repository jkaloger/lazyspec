---
title: "Symbol Extraction"
type: arch
status: accepted
author: jkaloger
date: 2026-03-15
tags: [architecture, engine, tree-sitter]
related:
  - related-to: "docs/stories/STORY-056-tree-sitter-symbol-extraction.md"
---

# Symbol Extraction

The `SymbolExtractor` trait provides language-specific code symbol extraction
powered by tree-sitter. Implemented in [STORY-056: Tree-sitter symbol extraction](../../stories/STORY-056-tree-sitter-symbol-extraction.md).

@ref src/engine/symbols.rs#SymbolExtractor

## Supported Languages and Symbols

**TypeScript** (`tree-sitter-typescript`):

@ref src/engine/symbols.rs#TypeScriptSymbolExtractor

**Rust** (`tree-sitter-rust`):

@ref src/engine/symbols.rs#RustSymbolExtractor

The extraction algorithm walks the tree-sitter CST recursively, matching nodes
by kind and checking the `name` (or `type` for impl blocks) field against the
requested symbol name. Returns the full source text span of the matching node.

@ref src/engine/symbols.rs#find_symbol_node

## Extension

New languages can be added by implementing the `SymbolExtractor` trait and
adding a match arm in `RefExpander::extract_symbol()` for the file extension.
