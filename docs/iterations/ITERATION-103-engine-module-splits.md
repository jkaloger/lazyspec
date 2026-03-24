---
title: Engine Module Splits
type: iteration
status: accepted
author: unknown
date: 2026-03-23
tags: []
related:
- implements: docs/stories/STORY-083-engine-module-splits.md
---



## Context

Implements STORY-083 (RFC-032 streams 5e and 5f). `store.rs` (604 lines) and `refs.rs` (425 lines) each combine multiple concerns. This iteration splits them into focused submodules using the Rust 2021 `file.rs` + `file/` subdirectory layout.

## Changes

### Split store.rs (stream 5e)

- [ ] Create `src/engine/store/` directory
- [ ] Extract `load`, `load_type_directory`, and `parse_document_entry` into `src/engine/store/loader.rs`
- [ ] Extract `forward_links`, `reverse_links`, `related_to`, and `referenced_by` into `src/engine/store/links.rs`
- [ ] Retain `Store` struct and query API (`list`, `get`, `resolve_shorthand`) in `store.rs`
- [ ] Add `mod loader;` and `mod links;` declarations in `store.rs`, re-exporting items consumed outside the module

### Split refs.rs (stream 5f)

- [ ] Create `src/engine/refs/` directory
- [ ] Extract `find_fenced_code_ranges` into `src/engine/refs/code_fence.rs`
- [ ] Extract `resolve_ref`, `resolve_head_short_sha`, and `language_from_extension` into `src/engine/refs/resolve.rs`
- [ ] Retain `RefExpander`, `expand`, and `expand_cancellable` in `refs.rs`
- [ ] Add `mod code_fence;` and `mod resolve;` declarations in `refs.rs`

### Update import paths

- [ ] Search for any `use crate::engine::store::` and `use crate::engine::refs::` references across `src/` and update to the new submodule paths where needed
- [ ] Verify no broken imports remain (`cargo check`)

## Test Plan

- [ ] `cargo check` passes with no errors after each module split (run incrementally: store first, then refs)
- [ ] `cargo test` passes without modifying any test assertions — only import paths should change
- [ ] Manual smoke test: `cargo run -- list` and `cargo run -- show <any-doc-id>` produce correct output

## Notes

The module splits are purely structural. No logic changes, no renamed functions, no altered signatures. If a test assertion needs changing (beyond import paths), that is a regression and should be investigated before merging.
