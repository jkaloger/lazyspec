---
title: "Project Status and Validation"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: []
related:
  - related-to: "docs/stories/STORY-020-status-command.md"
  - related-to: "docs/stories/STORY-022-expanded-validation.md"
---

## Summary

The `status` and `validate` commands provide two complementary views of project health. `status` gives a full inventory of documents with inline validation results. `validate` runs the validation pipeline in isolation and reports errors and warnings with appropriate exit codes for CI integration.

Both commands load the `Store` (@ref src/engine/store.rs#Store) and call `validate_full` (@ref src/engine/validation.rs#validate_full) against it, but they differ in output shape and intent.

## Status Command

`lazyspec status` produces a project overview. In human mode it groups documents by type in a fixed order: RFC, Story, Iteration, ADR. Within each group, documents are sorted by date via `DocMeta::sort_by_date`, which sorts ascending by date with path as tiebreaker. Each document is rendered as a card showing title, type, status, and path.

When no documents exist, `run_human` (@ref src/cli/status.rs#run_human) returns an empty string and the dispatcher prints "No documents found." with exit code 0.

JSON mode calls `run_json` (@ref src/cli/status.rs#run_json), which returns an object with three top-level keys: `documents`, `validation`, and `parse_errors`. The `documents` array contains one entry per document with all frontmatter fields serialised via `doc_to_json`. The `validation` object contains `errors` and `warnings` arrays produced by `validate_full`. The `parse_errors` array contains objects with `path` and `error` fields for any files that failed frontmatter parsing.

This design means JSON consumers get document inventory and validation results in a single call, without needing to invoke `validate` separately.

## Validate Command

`lazyspec validate` runs `validate_full` and reports the results. The entry point is `run_full` (@ref src/cli/validate.rs#run_full), which accepts a `json` flag and a `warnings` flag.

The exit code follows a simple rule: if `result.errors` is non-empty or `store.parse_errors()` is non-empty, exit 2; otherwise exit 0. Warnings never affect the exit code.

### Human Output

`run_human` (@ref src/cli/validate.rs#run_human) iterates parse errors first, then validation errors, then (if `show_warnings` is true) warnings. Each line is prefixed with a styled error or warning marker via `error_prefix` and `warning_prefix`. Output goes to stderr via `eprint!`. When no issues exist, a green checkmark and "All documents valid." is printed to stdout.

The `--warnings` flag controls whether warnings appear. Without it, only errors and parse errors are shown.

### JSON Output

`run_json` (@ref src/cli/validate.rs#run_json) returns an object with three arrays: `errors`, `warnings`, and `parse_errors`. Each error and warning is formatted as a display string. Parse errors carry `path` and `error` fields. The `--warnings` flag has no effect on JSON output since both arrays are always present, letting consumers filter as they choose.

## Validation Pipeline

Both commands delegate to `validate_full` (@ref src/engine/validation.rs#validate_full), which iterates a fixed list of checkers and merges their results into a single `ValidationResult` (@ref src/engine/validation.rs#ValidationResult). Each checker implements the `Checker` trait and returns `(Severity, ValidationIssue)` pairs. The severity determines whether an issue lands in the `errors` or `warnings` vec.

The current checker set is: `BrokenLinkRule`, `ParentLinkRule`, `StatusConsistencyRule`, `DuplicateIdRule`, `AcSlugFormatRule`, `RefScopeRule`, and `OrphanRefRule`. The issue variants cover broken links, missing parent links, superseded/rejected parents, orphaned acceptance, duplicate IDs, invalid AC slugs, ref count exceeded, cross-module refs, and orphan refs.

Severity is determined per-rule. For example, `StatusConsistencyRule` emits `SupersededParent` as a warning but `RejectedParent` as an error. Only errors (and parse errors) cause a non-zero exit code.
