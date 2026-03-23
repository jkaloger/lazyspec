---
title: DRY Consolidation
type: iteration
status: draft
author: unknown
date: 2026-03-23
tags: []
related:
- implements: docs/stories/STORY-080-dry-consolidation.md
---


## Overview

RFC-032 stream 3 (DRY Consolidation). Extracts shared helpers, adds `DocMeta::display_name()`, renames the two divergent `strip_type_prefix` functions, and adopts `canonical_name()` once STORY-079 delivers it. No user-facing behaviour changes.

## Tasks

### Stream 3b: DocMeta::display_name()

- [ ] In `src/engine/document.rs`, add `pub fn display_name(&self) -> &str` to `DocMeta`. Return `self.id` (already computed during load); this makes the ad-hoc path check unnecessary.
- [ ] In `src/cli/fix.rs`, remove `doc_display_name` helper and replace every call-site with `doc.display_name()`.
- [ ] In `src/engine/store.rs`, replace the `extract_id` path-checking logic in `resolve_shorthand` with `doc.display_name()`.
- [ ] Verify no other files duplicate the `index.md` path check for display-name purposes.

### Stream 3c: build_type_def() builder

- [ ] In `src/engine/config.rs` (or wherever `default_types` and `types_from_directories` live), add a private `fn build_type_def(name: &str, dir: &str, prefix: &str, icon: &str) -> TypeDef`. Set `plural` to `format!("{}s", name)` with the `story -> stories` irregular form handled explicitly.
- [ ] Refactor `default_types()` so each entry is a one-liner delegating to `build_type_def`.
- [ ] Refactor `types_from_directories()` likewise.
- [ ] Confirm `TypeDef` fields not set by the builder (e.g. `numbering`) retain correct defaults.

### Stream 3d: strip_type_prefix rename

- [ ] In `src/engine/store.rs`, rename `strip_type_prefix` to `strip_type_prefix_sqids`. Update all internal call-sites.
- [ ] In `src/cli/fix.rs`, rename `strip_type_prefix` to `strip_type_prefix_numeric`. Update all internal call-sites.
- [ ] Confirm the two functions are not re-exported or referenced across crate boundaries.

### Stream 3a: canonical_name() adoption (deferred on STORY-079)

- [ ] Once STORY-079 merges and `canonical_name(doc: &DocMeta) -> Option<&str>` is available, update both the qualified and unqualified branches of `resolve_shorthand()` to delegate to it, removing the duplicated closure logic.
- [ ] This task is a hard dependency on STORY-079; do not merge this iteration until that story is complete or scope it out to a follow-up.

## Test Plan

- [ ] `cargo test` passes with no regressions after each stream.
- [ ] `cargo clippy -- -D warnings` reports no new warnings.
- [ ] Run `cargo run -- fix --dry-run` on a local doc tree; confirm output is identical to pre-change behaviour.
- [ ] Manually verify `display_name()` returns correct values for both `index.md`-style and flat-file docs.
- [ ] Search for any remaining ad-hoc `index.md` path checks after the refactor (`grep -r "index.md" src/`) and confirm none relate to display name resolution.

## Notes

The `canonical_name()` task (stream 3a) depends on STORY-079. If that story is delayed, ship the other three streams independently and track 3a as a follow-up iteration.
