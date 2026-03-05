---
title: Engine and CLI Quality
type: story
status: draft
author: agent
date: 2026-03-05
tags: [refactor, quality, testing]
related:
- implements: docs/rfcs/RFC-009-codebase-quality-baseline.md
---


## Context

Code review identified duplicated logic, dead code, missing trait impls, and test infrastructure gaps across the engine and CLI layers. These are internal quality issues that increase the cost of future changes.

## Acceptance Criteria

- **Given** frontmatter splitting logic exists in `document.rs`
  **When** any module needs to split frontmatter
  **Then** it uses the single shared implementation (no file-local copies)

- **Given** `validate_full()` supersedes `validate()`
  **When** the codebase is searched for validation entry points
  **Then** only `validate_full()` exists (no dead `validate()` / `ValidationError`)

- **Given** `DocType`, `Status`, and `RelationType` need to be parsed from strings
  **When** any module parses these types
  **Then** it uses `FromStr` trait impls (no manual match blocks on string literals)

- **Given** `nucleo` and `pulldown-cmark` are not imported anywhere
  **When** dependencies are audited
  **Then** they are not listed in Cargo.toml

- **Given** `main.rs` dispatches CLI commands
  **When** shared setup (cwd, config, store) is needed
  **Then** each value is computed at most once

- **Given** the TUI event loop dispatches key events
  **When** a key is pressed in any mode
  **Then** the handler is a method on `App` (not inline in the event loop)

- **Given** `update.rs` modifies a scalar frontmatter field
  **When** the file is written back
  **Then** all other frontmatter fields retain their original formatting

- **Given** `store.rs` contains validation logic
  **When** the module is reviewed
  **Then** validation lives in a separate `validation.rs` module

- **Given** test files need Store fixtures
  **When** a test is written
  **Then** shared helpers in `tests/common/mod.rs` are available

## Scope

### In Scope

- ITERATION-013: Engine Cleanup
- ITERATION-014: CLI and TUI Cleanup
- ITERATION-015: Test Infrastructure
- ITERATION-016: Validation Module Extraction

### Out of Scope

- New user-facing features
- TUI test coverage (separate story)
- Fuzzy search implementation
