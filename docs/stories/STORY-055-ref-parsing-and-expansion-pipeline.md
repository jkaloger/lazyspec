---
title: Ref parsing and expansion pipeline
type: story
status: draft
author: agent
date: 2026-03-11
tags: []
related:
  - implements: docs/rfcs/RFC-019-inline-type-references-with-ref.md
---


## Context

Implements the ref parsing and expansion pipeline for RFC-019. This story handles parsing `@ref` directives from markdown bodies and replacing them with fenced code block placeholders. The actual symbol extraction and content population are delegated to subsequent stories.

## Acceptance Criteria

- **Given** a document body containing `@ref src/foo.rs#MyStruct`
  **When** `get_body()` is called
  **Then** the directive is replaced with a fenced code block containing the extracted type

- **Given** a document body containing `@ref src/utils.ts#SomeInterface@abc1234`
  **When** `get_body()` is called
  **Then** the directive resolves against git commit `abc1234` instead of HEAD

- **Given** a document body containing multiple `@ref` directives
  **When** `get_body()` is called
  **Then** all directives are replaced with fenced code blocks in order

- **Given** a `@ref` directive with `.ts` extension
  **When** the directive is expanded
  **Then** the language tag is set to `ts`

- **Given** a `@ref` directive with `.rs` extension
  **When** the directive is expanded
  **Then** the language tag is set to `rust`

- **Given** a `@ref` directive with `.py` extension
  **When** the directive is expanded
  **Then** the language tag is set to `python`

- **Given** a `@ref src/path.rs#SymbolName` directive
  **When** the directive is parsed
  **Then** the file path `src/path.rs` and symbol `SymbolName` are extracted correctly

## Scope

### In Scope

- Parse `@ref` directives from markdown bodies using regex
- Implement `expand_refs()` as a transform step in `get_body()`
- Resolve file content via `git show HEAD:<path>` for HEAD refs
- Resolve file content via `git show <sha>:<path>` for SHA-scoped refs
- Extract the file path and symbol from the directive
- Derive language tag from file extension (.ts -> ts, .rs -> rust, etc.)
- Replace `@ref` with fenced code block placeholder (symbol extraction delegated to next story)
- Handle multiple `@ref` directives in same document

### Out of Scope

- Tree-sitter symbol extraction (delegated to story 2)
- Actual code block content population
- Rendering integration in CLI/TUI (delegated to story 3)
- Error handling for unresolved refs (delegated to story 3)

(End of file - total 53 lines)
