---
title: Rendering integration
type: story
status: accepted
author: agent
date: 2026-03-11
tags: []
related:
- implements: RFC-019
---




## Context

This story integrates the ref expansion pipeline into the rendering layer, enabling `@ref` directives to expand in both CLI and TUI outputs.

## Acceptance Criteria

- **Given** a document containing `@ref src/types/user.ts#UserProfile`
  **When** `lazyspec show` is run on the document
  **Then** the ref is replaced with a fenced code block containing the extracted TypeScript type definition

- **Given** a document containing `@ref src/types/user.ts#UserProfile`
  **When** the document is viewed in the TUI preview
  **Then** the ref is replaced with a fenced code block containing the extracted TypeScript type definition

- **Given** a ref with a valid git SHA `@ref src/types/user.ts#UserProfile@a1b2c3d`
  **When** the document is rendered
  **Then** the type is extracted from the file at that specific commit

- **Given** a ref to a non-existent file `@ref nonexistent.ts#Foo`
  **When** the document is rendered
  **Then** a warning block appears: `> ⚠️ [unresolved: nonexistent.ts#Foo]` with message about file not found

- **Given** a ref to a non-existent symbol `@ref src/types/user.ts#NonExistent`
  **When** the document is rendered
  **Then** a warning block appears with message about symbol not found

- **Given** a ref with an invalid git SHA `@ref src/types/user.ts#UserProfile@invalid_sha`
  **When** the document is rendered
  **Then** a warning block appears with message about invalid SHA

- **Given** a ref to a TypeScript file `@ref src/types/user.ts#UserProfile`
  **When** the document is rendered
  **Then** the code fence uses `ts` as the language tag

- **Given** a ref to a Rust file `@ref src/models/user.rs#User`
  **When** the document is rendered
  **Then** the code fence uses `rust` as the language tag

- **Given** a document with both resolved and unresolved refs
  **When** the document is rendered
  **Then** resolved refs show as code blocks and unresolved refs show as warning blocks, with the rest of the document rendering normally

## Scope

### In Scope

- Wire tree-sitter symbol extraction into expand_refs() pipeline
- Populate fenced code blocks with extracted type content
- Integrate ref expansion into CLI `lazyspec show` output
- Integrate ref expansion into TUI document preview
- Detect language from file extension for code fence tags
- Render unresolved refs as warning blocks with helpful error messages
- Handle errors: file not found, symbol not found, bad SHA, git unavailable

### Out of Scope

- CLI flag gating (`-e`/`--expand-references`)
- TUI lazy loading / async expansion
- Caching of expanded bodies

## Implementation Notes

Shipped in commit 9d2a03b. Integration was done by wiring `expand_refs()` into `Store::get_body()`. The expansion runs synchronously on every call including TUI render frames. Warning format diverges from spec (uses HTML comments instead of blockquote format). Integration tests live in `tests/expand_refs_test.rs`.
