---
title: "Ref Directives"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: [refs, engine, expansion]
related:
  - related-to: "docs/stories/STORY-055-ref-parsing-and-expansion-pipeline.md"
  - related-to: "docs/stories/STORY-057-rendering-integration.md"
  - related-to: "docs/stories/STORY-058-ref-expansion-hardening-and-performance.md"
---

## Acceptance Criteria

### AC: parse-path-only-ref

Given a document body containing `@ref src/foo.rs`
When the ref is parsed by `REF_PATTERN`
Then capture group 1 yields `src/foo.rs`, and groups 2 and 3 are absent

### AC: parse-symbol-ref

Given a document body containing `@ref src/foo.rs#MyStruct`
When the ref is parsed by `REF_PATTERN`
Then capture group 1 yields `src/foo.rs` and group 2 yields `MyStruct`

### AC: parse-sha-pinned-ref

Given a document body containing `@ref src/foo.rs#MyStruct@abc1234`
When the ref is parsed by `REF_PATTERN`
Then group 1 yields `src/foo.rs`, group 2 yields `MyStruct`, and group 3 yields `abc1234`

### AC: expand-whole-file

Given a `@ref Cargo.toml` directive in a document
When `RefExpander::expand` is called
Then the directive is replaced with a fenced code block containing the full file content from the HEAD revision

### AC: expand-named-symbol

Given a `@ref src/engine/store.rs#Store` directive
When `RefExpander::expand` is called
Then the directive is replaced with a fenced code block containing the `Store` symbol definition extracted via tree-sitter

### AC: expand-line-number

Given a `@ref Cargo.toml#1` directive
When `RefExpander::expand` is called
Then the directive is replaced with a fenced code block containing exactly one line (the first line of `Cargo.toml`)

### AC: unresolved-missing-file

Given a `@ref nonexistent/file.rs` directive referencing a file that does not exist in the repo
When `RefExpander::expand` is called
Then the directive is replaced with `> [unresolved: nonexistent/file.rs]`

### AC: unresolved-missing-symbol

Given a `@ref Cargo.toml#NonExistentSymbol` directive where the symbol is not found
When `RefExpander::expand` is called
Then the directive is replaced with `> [unresolved: Cargo.toml#NonExistentSymbol]` and the full file content is not dumped

### AC: skip-refs-inside-fences

Given a document where `@ref src/foo.rs` appears inside an existing fenced code block
When `RefExpander::expand` is called
Then that directive is left untouched and not expanded

### AC: truncation-at-max-lines

Given a `@ref` that resolves to content exceeding `max_lines` (default 25)
When the expansion is rendered
Then only the first `max_lines` lines appear, followed by a language-appropriate truncation comment indicating the remaining line count

### AC: language-tag-mapping

Given a `@ref` pointing to a `.rs` file
When the expansion is rendered
Then the fenced code block uses `rust` as its language tag, and similarly `.ts`/`.tsx` maps to `ts`, `.py` to `python`, and unrecognized extensions to an empty tag

### AC: cancellable-expansion-returns-none

Given an `AtomicBool` cancel flag set to `true`
When `RefExpander::expand_cancellable` is called
Then the method returns `Ok(None)` without resolving any refs

### AC: disk-cache-hit-skips-git

Given a document whose body hash matches an existing `DiskCache` entry
When the TUI requests expansion
Then the cached expansion is returned without spawning git or tree-sitter work

### AC: disk-cache-invalidation-on-body-change

Given a cached expansion for a document
When the document body changes (producing a different body hash)
Then `DiskCache::read` returns `None` for the new hash, and a fresh expansion is performed

### AC: head-sha-resolved-once-per-pass

Given a document containing multiple `@ref` directives
When `RefExpander::expand` is called
Then `git rev-parse --short HEAD` is invoked once and the same short SHA is used in all captions
