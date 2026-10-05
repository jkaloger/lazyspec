---
title: Rust symbol extraction omits leading attributes and doc comments
type: bug
status: reported
author: Jack Kaloger
date: 2026-10-05
tags: []
related:
- related-to: RFC-019
- related-to: STORY-056
---

## Summary

Rust symbol extraction returns the item's byte range only. Leading attributes and doc comments are omitted, so changing `#[should_panic]` does not change the extracted text.

## Reproduction

1. `@ref src/foo.rs#my_test` where `my_test` carries `#[test]`.
2. Add `#[should_panic]` above the function.
3. Extracted text and `lazyspec pin` blob hash are unchanged.

## Expected

Attribute and doc-comment changes change the extracted text and hash.

## Actual

`find_symbol_node` (`src/engine/symbols.rs:62`) returns the `function_item` range. tree-sitter-rust parses `#[test]`, `#[should_panic]`, `#[ignore]` as preceding sibling `attribute_item` nodes, outside that range.

Affects `lazyspec pin` blob hashes and lemma's check, which compares extracted test text.

## Fix direction

Follow Rust semantics. Walk back over preceding siblings while they are `attribute_item`, an outer doc comment (`///`, `/** */`), or a plain comment, ignoring blank lines. Stop at anything else: another item, an inner attribute, an inner doc comment. Then trim leading plain comments, so the range starts at the earliest attribute or outer doc comment. With no attribute or doc found, start at the item.

- Outer docs only. Inner `//!` and `/*! */` belong to the enclosing module and are not absorbed.
- Attributes and docs attach across blank lines and interleaved plain `//` comments (`#[should_panic]\n\nfn t()` includes the attribute).
- A plain `//` comment alone above a fn is excluded.

## Consequence

Normalized hashing (default `normalize=true`, `normalize_ast` strips comments) means doc-comment edits change the extracted text but not the pinned hash, while attribute changes do change it. Intended: doc comments do not change what a test proves.


Every existing pinned Rust `@ref` blob hash changes once. Needs a release note telling users to re-run `lazyspec pin`.

## Related

`@ref`: RFC-019. Symbol extraction: SPEC-006.
