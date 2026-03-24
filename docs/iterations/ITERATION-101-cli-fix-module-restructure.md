---
title: CLI Fix Module Restructure
type: iteration
status: accepted
author: unknown
date: 2026-03-23
tags: []
related:
- implements: docs/stories/STORY-082-cli-fix-module-restructure.md
---



## Changes

Implements RFC-032 streams 4b and 5d. `cli/fix.rs` is 1043 lines spanning field fixing, conflict resolution, renumbering, reference scanning, and output formatting. This iteration renames two functions for clarity and splits the module into focused submodules.

### Function Renames (Stream 4b)

- [ ] Rename `collect_renumber_fixes` to `plan_renumbering` in `src/cli/fix.rs`
- [ ] Rename `collect_all` to `plan_field_and_conflict_fixes` in `src/cli/fix.rs`
- [ ] Update all call sites referencing the old names

### Module Split (Stream 5d)

Target structure:

```
src/cli/
├── fix.rs           (module root: run(), run_json() entry points only)
└── fix/
    ├── fields.rs    (field fix collection and application)
    ├── conflicts.rs (duplicate ID detection and resolution)
    ├── renumber.rs  (renumbering orchestration, reference cascade)
    └── output.rs    (JSON and human-readable output formatting)
```

- [ ] Create `src/cli/fix/` directory and stub submodule files
- [ ] Move field fix collection and application logic to `fix/fields.rs`
- [ ] Move duplicate ID detection and resolution logic to `fix/conflicts.rs`
- [ ] Move `plan_renumbering` and reference cascade logic to `fix/renumber.rs`
- [ ] Move JSON and human-readable formatting logic to `fix/output.rs`
- [ ] Reduce `fix.rs` to module root: declare submodules, expose `run()` and `run_json()`
- [ ] Update all `use crate::cli::fix::*` import paths across the codebase
- [ ] Verify `src/cli/mod.rs` still resolves `fix` correctly under the new layout

## Test Plan

- [ ] `cargo test` passes without modifying any test assertions (only import paths change)
- [ ] `cargo run -- fix --help` produces unchanged output
- [ ] `cargo run -- fix --dry-run` on a test fixture produces identical output to pre-split
- [ ] `cargo run -- fix --json` on a test fixture produces identical JSON to pre-split
- [ ] `cargo clippy` reports no new warnings

## Notes

The renames are purely semantic: "plan" conveys that these functions produce a list of intended changes without applying them, matching the existing dry-run semantics. No logic changes.

The module split has no behaviour impact. All public entry points (`run`, `run_json`) remain in `fix.rs`. Internal visibility between submodules uses `pub(super)` where needed.
