---
title: "Root-file Module Pattern Migration"
type: iteration
status: accepted
author: "jkaloger"
date: 2026-03-24
tags: []
related: []
---


Migrate all `mod.rs` files to the Rust 2018+ root-file pattern (`foo.rs` + `foo/` instead of `foo/mod.rs`). Addresses AUDIT-008 F1 (mixed module patterns), F2 (thick fix module root), and F3 (visibility inconsistency). Purely structural, no behaviour changes.

## Changes

### Task 1: Migrate engine/mod.rs to engine.rs

Files:
- Move: `src/engine/mod.rs` -> `src/engine.rs`

Move `src/engine/mod.rs` to `src/engine.rs` (sibling to the `engine/` directory). The file is 11 lines of `pub mod` declarations, so no content changes are needed. Verify `cargo build` passes.

### Task 2: Migrate cli/mod.rs to cli.rs

Files:
- Move: `src/cli/mod.rs` -> `src/cli.rs`

Move `src/cli/mod.rs` (202 lines, contains the `Cli` struct, `Commands` enum, and all subcommand arg definitions) to `src/cli.rs`. No content changes. Verify `cargo build` passes.

### Task 3: Migrate cli/fix/mod.rs to cli/fix.rs

Files:
- Move: `src/cli/fix/mod.rs` -> `src/cli/fix.rs`

Move `src/cli/fix/mod.rs` (203 lines, the fix command dispatcher) to `src/cli/fix.rs`. While here, normalise visibility: change `pub mod renumber` to `mod renumber` to match the other three private sub-modules (`mod conflicts`, `mod fields`, `mod output`), unless `renumber` is imported externally. Grep for `use.*cli::fix::renumber` outside of `src/cli/fix/` to confirm. Verify `cargo build` passes.

### Task 4: Migrate tui/mod.rs to tui.rs

Files:
- Move: `src/tui/mod.rs` -> `src/tui.rs`

Move `src/tui/mod.rs` (308 lines, contains the event loop and terminal lifecycle) to `src/tui.rs`. No content changes in this iteration; the event loop extraction happens in ITERATION-108. Verify `cargo build` passes.

## Test Plan

- `cargo build` passes after each task with zero warnings
- `cargo build --all-features` passes after all four tasks
- `cargo test` passes (full suite)
- Verify no `mod.rs` files remain under `src/` after all tasks: `find src -name mod.rs` returns empty
- Verify all import paths resolve without changes (the root-file pattern is transparent to consumers)

## Notes

Each task is a single `git mv` plus an optional visibility fix (task 3). These should be separate commits so any import resolution issues are isolated. The order doesn't matter since the four modules are independent, but doing engine first (simplest, 11 lines) is a good smoke test.
