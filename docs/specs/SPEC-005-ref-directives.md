---
title: "Ref Directives"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: [refs, engine, expansion]
related:
  - related-to: "docs/architecture/ARCH-002-data-model/ref-directives.md"
  - related-to: "docs/architecture/ARCH-003-engine/ref-expansion.md"
  - related-to: "docs/architecture/ARCH-003-engine/cache-and-template.md"
---

## Summary

Spec documents and other markdown bodies can reference source code inline using `@ref` directives. Rather than pasting code into documents (where it rots), authors write a short directive and the engine expands it at display time against a git revision. The raw directive is stored; the expansion is ephemeral.

## Syntax

Ref directives are parsed by a single regex defined as a module-level constant:

@ref src/engine/refs.rs#REF_PATTERN

The pattern decomposes each directive into three capture groups: a file path (required), a symbol or line number after `#` (optional), and a hex SHA after `@` (optional). This yields four practical forms:

- `@ref src/foo.rs` resolves the entire file.
- `@ref src/foo.rs#MyStruct` resolves a named symbol within the file.
- `@ref src/foo.rs#42` resolves line 42 of the file.
- `@ref src/foo.rs#MyStruct@abc1234` resolves a symbol at a pinned git commit.

Path segments may not contain `#`, `@`, or whitespace. SHA fragments must be hexadecimal.

## The RefExpander

@ref src/engine/refs.rs#RefExpander

`RefExpander` holds a repository root path and a `max_lines` ceiling (default 25). Two constructors exist: `new` sets the default, `with_max_lines` allows callers to override. The TUI path uses the default; the CLI `get_body_expanded` on `Store` passes a caller-chosen limit.

### Expansion Pipeline

The `expand` method drives a straightforward pipeline. It compiles the regex, identifies all matches in the input, skips any that fall inside existing fenced code blocks, resolves each match through git, and applies replacements in reverse offset order so that earlier substitutions do not invalidate later byte positions.

@ref src/engine/refs/resolve.rs#resolve_ref

Resolution shells out to `git show {rev}:{path}`, where `rev` defaults to `HEAD` when no SHA is pinned. If git fails (missing file, bad SHA, unreachable rev), the directive is replaced with a blockquote marker: `> [unresolved: path#symbol]`. This is a non-fatal fallback; the rest of the document expands normally.

When a symbol fragment is present, the resolver branches on whether it is all-ASCII-digit. Numeric fragments are treated as 1-indexed line numbers and produce a single-line expansion. Non-numeric fragments are routed through language-specific symbol extractors (Rust and TypeScript are supported via tree-sitter). If the extractor returns nothing, the unresolved marker is emitted.

### Output Format

Each resolved directive expands into a caption line followed by a fenced code block:

```
**path** @ `sha` (symbol)
```lang
<content>
```
```

The caption includes the file path, the short SHA (resolved once per expansion pass via `git rev-parse --short HEAD`), and a parenthetical showing the symbol name or line number. When no SHA is pinned, the current HEAD short hash is displayed.

### Language Detection

@ref src/engine/refs/resolve.rs#language_from_extension

The `language_from_extension` function maps file extensions to code fence language tags. It covers TypeScript (`.ts`/`.tsx` to `ts`), JavaScript, Rust, Python, Go, Java, C, C++, Markdown, JSON, YAML, and TOML. Unrecognized extensions produce an empty string.

### Truncation

When the resolved content exceeds `max_lines`, only the first `max_lines` lines are kept and a language-appropriate truncation comment is appended. Python, YAML, and TOML use `#` comments; Markdown uses HTML comments; everything else uses `//`.

@ref src/engine/refs/resolve.rs#truncation_comment

## Code Fence Detection

@ref src/engine/refs/code_fence.rs#find_fenced_code_ranges

Refs that appear inside existing fenced code blocks must not be expanded (this prevents recursive expansion of previously-expanded output). The `find_fenced_code_ranges` function scans content byte-by-byte for triple-backtick fence pairs and returns their byte ranges. An unclosed fence extends to EOF.

@ref src/engine/refs/code_fence.rs#is_inside_fence

`is_inside_fence` checks whether a given byte offset falls within any fenced range. The `expand` method calls this for every regex match and skips those inside fences.

## Cancellable Expansion

The TUI needs to expand refs in a background thread and discard results when the user navigates away. `expand_cancellable` mirrors `expand` but accepts an `AtomicBool` flag. Before resolving each match, it checks the flag and returns `Ok(None)` if cancellation has been signalled. This avoids wasted git calls when the user switches documents quickly.

@ref src/engine/refs.rs#expand_cancellable

The TUI integration in `App::request_expansion` spawns a thread, passes a cancellation token, and stores the token so that a subsequent navigation can set it. When the expansion thread returns `None`, the result is silently dropped.

## DiskCache

@ref src/engine/cache.rs#DiskCache

Expanded bodies are cached on disk at `~/.lazyspec/cache/` to avoid repeated git and tree-sitter work. Cache keys combine three components: a version constant (`CACHE_VERSION`, currently 3), a hash of the document path, and a hash of the raw body content. The key format is `v{VERSION}_{PATH_HASH}_{BODY_HASH}`.

This scheme provides automatic invalidation: when the document body changes, the body hash changes, so the old cache entry is never read. The version constant allows the format to evolve without stale entries causing problems.

Four operations are exposed: `read` returns a cached expansion or `None`, `write` stores an expansion, `invalidate` removes all entries for a given path (by scanning filenames for the path hash substring), and `clear` removes every entry in the cache directory.

The TUI checks the cache before spawning an expansion thread. If a cache hit occurs, the expanded body is sent directly to the UI without any git calls.

## Symbol Extraction

The `extract_symbol` method on `RefExpander` dispatches to language-specific extractors based on file extension. Only `.ts`/`.tsx` (TypeScript) and `.rs` (Rust) are currently supported. Other extensions cause the symbol lookup to return `None`, which produces the unresolved marker. The extractors use tree-sitter grammars to locate named definitions in the source text.
