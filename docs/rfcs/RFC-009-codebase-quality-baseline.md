---
title: "Codebase Quality Baseline"
type: rfc
status: draft
author: "agent"
date: 2026-03-05
tags: [refactor, quality, testing]
---

## Summary

A full code review surfaced structural debt across the engine, CLI, and TUI layers. The codebase works correctly but has accumulated duplication, dead code, and missing test coverage as features were added rapidly.

This RFC defines a quality baseline: a set of refactoring and testing work that reduces maintenance risk without changing user-facing behavior. All changes are internal.

## Intent

The codebase grew from ~0 to ~3.2k lines of source (plus ~2.1k lines of tests) over a short period. Feature velocity was prioritised over consolidation. The result is functional but has predictable debt:

- Shared logic duplicated across modules (frontmatter parsing appears 4 times)
- Dead code left behind after superseding implementations (`validate()` / `ValidationError`)
- String-based dispatching where type-safe parsing should exist (`DocType`, `Status`, `RelationType` parsed from strings in 5+ manual match blocks)
- A 500-line `store.rs` that handles loading, querying, search, and validation
- 17 test files with no shared fixtures
- 19 of 36 public TUI `App` methods untested

None of these are bugs. They're friction points that compound as the codebase grows.

## Scope

Five work streams, ordered by dependency:

### 1. Engine Cleanup
Consolidate frontmatter parsing, remove dead validation code, add `FromStr` trait impls, drop unused dependencies. Lowest risk, highest immediate payoff.

### 2. CLI and TUI Cleanup
Eliminate boilerplate in `main.rs`, extract TUI key handlers into testable `App` methods, fix YAML round-trip formatting destruction in `update.rs`.

### 3. Test Infrastructure
Create shared test fixtures (`tests/common/mod.rs`) to replace the duplicated setup patterns across 17 test files.

### 4. Validation Module Extraction
Move validation logic out of `store.rs` into `src/engine/validation.rs`. Keeps store focused on data loading and querying.

### 5. TUI Test Coverage
Cover the 19 untested `App` methods, particularly search, scrolling, relations navigation, and the new `handle_key` dispatch (added by stream 2).

## Out of Scope

- No new user-facing features
- No API changes (all public interfaces preserved)
- No changes to document format or frontmatter schema
- Fuzzy search (using the unused `nucleo` dep) is a feature, not a refactor

## Risks

Low. Every change is behavior-preserving. The existing test suite is the verification for streams 1-4. Stream 5 adds new tests.

The main risk is streams 1 and 2 touching many files. Keeping them as separate, ordered iterations mitigates this.

## Stories

This RFC decomposes into two Stories:
- **Engine and CLI Quality** (streams 1, 2, 3, 4): consolidation and infrastructure
- **TUI Test Coverage** (stream 5): coverage for untested App methods
