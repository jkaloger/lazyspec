---
title: Ref expansion hardening and performance
type: story
status: accepted
author: jkaloger
date: 2026-03-12
tags: []
related:
- implements: docs/rfcs/RFC-019-inline-type-references-with-ref.md
---



## Context

RFC-019's initial implementation (commit 9d2a03b) shipped all three stories in one commit. The code works but has bugs, naming issues, missing CLI flags, and synchronous TUI rendering that will freeze on large documents. This story covers the cleanup, CLI gating, and async TUI expansion.

## Acceptance Criteria

- **Given** a TypeScript or Rust source file with multiple type definitions
  **When** the symbol extractor is called
  **Then** it finds the correct symbol without skipping any (no double-advance bug)

- **Given** `lazyspec show RFC-001`
  **When** the document contains `@ref` directives
  **Then** refs are shown raw (unexpanded) by default

- **Given** `lazyspec show -e RFC-001`
  **When** the document contains `@ref` directives
  **Then** refs are expanded into fenced code blocks

- **Given** `lazyspec search "query"`
  **When** documents contain `@ref` directives
  **Then** search uses raw bodies (no git calls during search)

- **Given** an unresolvable `@ref` directive
  **When** expansion runs
  **Then** the output is `> [unresolved: path#symbol]` (blockquote format, not HTML comment)

- **Given** a document with `@ref` directives viewed in the TUI
  **When** the document is first selected
  **Then** the raw body is shown immediately with a loading indicator, and the expanded body replaces it once ready

- **Given** the user switches documents in the TUI
  **When** the previous document was still expanding
  **Then** the stale expansion is discarded and the new document begins loading

- **Given** ref expansion logic
  **When** inspecting module structure
  **Then** expansion lives in `src/engine/ref_expander.rs`, not as methods on `Store`

- **Given** the `./skills/build/SKILL.md` and `./skills/write-rfc/SKILL.md` files
  **When** an agent reads them
  **Then** they contain guidance on `@ref` syntax and the `-e` flag

## Scope

### In Scope

- Fix duplicate `visit_node` logic in symbol extractors
- Fix double-advance sibling bug in tree cursor walk
- Align test regex with production regex
- Remove debug eprintln from tests
- Extract ref expansion from Store into `ref_expander.rs` module
- Fix warning format to match spec (blockquote, not HTML comment)
- Add `-e`/`--expand-references` flag to CLI `show`
- Split `get_body` into raw and expanded variants
- Fix search to use raw body
- Update README with `-e` flag and `@ref` syntax docs
- Async TUI body expansion with background thread + cache
- Loading indicator in TUI preview while expanding
- Cache invalidation on document switch and file watch events
- Update skills (build, write-rfc, resolve-context) with `@ref` and `-e` guidance

### Out of Scope

- Additional language grammars (Python, Go, etc.)
- @ref validation rules (checking refs resolve at validate time)
- TUI-specific expand toggle
