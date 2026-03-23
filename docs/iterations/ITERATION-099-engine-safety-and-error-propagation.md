---
title: Engine Safety and Error Propagation
type: iteration
status: draft
author: unknown
date: 2026-03-23
tags: []
related:
- implements: docs/stories/STORY-079-engine-safety-and-error-propagation.md
---


## Changes

Implements RFC-032 streams 1, 2a, 2b, and 2c. Replaces panicking calls in the sqids pipeline and TUI cache lookups with proper error propagation, then flattens three over-nested engine functions by extracting focused helpers.

### Stream 1: Error Propagation

- [ ] In `engine/template.rs`: replace sqids builder `.expect()` and encode `.expect()` with `?` propagation
- [ ] In `cli/create.rs`: same sqids `.expect()` replacements; confirm function already returns `Result`
- [ ] In `cli/fix.rs`: same sqids `.expect()` replacements
- [ ] In `cli/reservations.rs`: same sqids `.expect()` replacements
- [ ] In `tui/app.rs`: audit `filtered_docs_cache` and other cache lookups using `.unwrap()`; replace with `.unwrap_or_default()` or match expressions

### Stream 2a: Flatten `validate_full()`

- [ ] Extract `validate_broken_links(store: &Store) -> Vec<ValidationIssue>` from `validate_full()`
- [ ] Extract `validate_parent_links(store: &Store, config: &Config) -> Vec<ValidationIssue>`
- [ ] Extract `validate_status_consistency(store: &Store) -> Vec<ValidationIssue>`
- [ ] Extract `validate_duplicate_ids(store: &Store) -> Vec<ValidationIssue>`
- [ ] Rewrite `validate_full()` as an orchestrator that calls each helper and collects results
- [ ] Each helper uses early `continue` to skip irrelevant documents and returns a flat `Vec`

### Stream 2b: Flatten `Store::load()`

- [ ] Extract `load_type_directory(root: &Path, type_def: &TypeDef) -> Result<Vec<DocMeta>>` to handle per-type entry iteration
- [ ] Extract `parse_document_entry(path: &Path, type_def: &TypeDef) -> Result<DocMeta>` for the read-parse-validate pipeline on a single file
- [ ] Rewrite `Store::load()` to call `load_type_directory()` per type; keep virtual doc creation and link graph building in `load()` since they depend on the full document set

### Stream 2c: Flatten `resolve_shorthand()`

- [ ] Extract `canonical_name(doc: &DocMeta) -> Option<&str>` that returns the parent directory name when the path ends in `index.md` or `.virtual`, otherwise returns the filename stem
- [ ] Refactor the qualified and unqualified branches of `resolve_shorthand()` to call `canonical_name()` instead of duplicating the 3-level nested closure

## Test Plan

- [ ] `cargo build` passes with no new warnings after each stream
- [ ] `cargo test` passes after stream 1; confirm no regressions in existing tests
- [ ] Manual smoke test: `cargo run -- create rfc "Test"` creates a document with a valid sqids ID
- [ ] Manual smoke test: `cargo run -- fix --dry-run` completes without panic
- [ ] Manual smoke test: `cargo run -- validate` runs all four validation helpers and produces the same issues as before the refactor
- [ ] Manual smoke test: `cargo run -- show STORY-079` resolves shorthand and returns the correct document
- [ ] TUI: open the TUI, navigate the doc list, confirm no crash when cache is empty on startup

## Notes

All changes are signature-preserving or internal. No user-facing behaviour changes, no frontmatter schema changes. The sqids `?` propagation requires callers to handle the error, but all affected functions already return `Result`.
