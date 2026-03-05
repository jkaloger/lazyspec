---
title: Validation Module Extraction
type: iteration
status: draft
author: agent
date: 2026-03-05
tags: []
related:
- implements: docs/stories/STORY-028-engine-and-cli-quality.md
---




## Problem

`store.rs` handles data loading, querying, search, and validation. After ITERATION-013 removes dead validation code, validation still accounts for ~130 lines of logic plus the `ValidationIssue` enum, `ValidationResult` struct, and Display impl. This is a natural module boundary.

## Changes

Stub. Full task breakdown to be written when this iteration is picked up. High-level scope:

- Create `src/engine/validation.rs`
- Move `validate_full()` logic, `ValidationIssue`, `ValidationResult`, and the Display impl
- The validation function needs read access to `Store` internals (`docs`, `reverse_links`). Options: make those fields `pub(crate)`, pass them as arguments, or keep `validate_full` as a method on Store that delegates to validation module functions.
- Update imports in `cli/validate.rs`, `cli/status.rs`, and test files

## Test Plan

Existing test suite is the verification. Pure move refactor.

## Notes

- Depends on ITERATION-013 (removing dead `validate()` / `ValidationError` first simplifies what needs to move).
- Design decision needed: how validation accesses Store internals. Recommend `pub(crate)` fields since they're already effectively public through the validation methods.
