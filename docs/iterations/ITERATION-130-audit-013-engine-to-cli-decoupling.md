---
title: AUDIT-013 engine-to-CLI decoupling
type: iteration
status: accepted
author: agent
date: 2026-03-28
tags: []
related:
- related-to: AUDIT-013
- blocks: ITERATION-131
---




## Changes

### Task 1: Extract filesystem document creation into the engine layer

Audit finding 9. `FilesystemStore::create` at `src/engine/store_dispatch.rs:58` calls `crate::cli::create::run`, creating a circular dependency (engine -> CLI).

Move the core logic from `src/cli/create.rs::run` (lines 16-118) into a new engine function. The natural home is a new module `src/engine/fs_store.rs` or directly in `store_dispatch.rs`. The function should accept the same parameters (root, config, type_def, title, author, on_progress callback) and return `Result<PathBuf>`.

What stays in `cli/create.rs`: argument parsing, JSON output formatting (`run_json`), the `load_template` and `story_template` helpers (or move those too if they have no CLI dependency).

Update `FilesystemStore::create` to call the engine function. Update `cli/create.rs::run` to delegate to the same engine function (thin wrapper).

AC: `FilesystemStore::create` no longer imports from `crate::cli`. `cargo test` passes.

### Task 2: Extract filesystem document update into the engine layer

`FilesystemStore::update` at `src/engine/store_dispatch.rs:88` calls `crate::cli::update::run`.

`cli/update.rs::run` (line 14) takes `(root, store, doc_path, updates)`. Move the core frontmatter-patching logic into an engine function. The CLI wrapper handles argument parsing and `--body`/`--body-file` flags.

Update `FilesystemStore::update` to call the engine function directly.

AC: `FilesystemStore::update` no longer imports from `crate::cli`. `cargo test` passes.

### Task 3: Extract filesystem document deletion into the engine layer

`FilesystemStore::delete` at `src/engine/store_dispatch.rs:97` calls `crate::cli::delete::run`.

`cli/delete.rs::run` (line 13) takes `(root, store, doc_path)`. Move the core deletion logic (resolve path, remove file, clean empty dirs) into an engine function. CLI wrapper handles argument parsing and confirmation prompts.

Update `FilesystemStore::delete` to call the engine function directly.

AC: `FilesystemStore::delete` no longer imports from `crate::cli`. `cargo test` passes.

### Task 4: Verify no engine-to-CLI imports remain

Grep `src/engine/` for any `crate::cli::` imports. There should be zero. If any remain (e.g. in `store_dispatch.rs` or elsewhere), fix them.

Run `cargo test` to confirm no regressions.

AC: `grep -r 'crate::cli' src/engine/` returns zero results. Full test suite passes.

## Test Plan

- `cargo test` passes after each task
- `grep -r 'crate::cli' src/engine/` returns nothing after task 4
- CLI commands (`lazyspec create`, `lazyspec update`, `lazyspec delete`) still work identically for both filesystem and github-issues types
- Existing integration tests in `store_dispatch.rs` pass without modification

## Notes

This iteration blocks ITERATION-131 (store internals cleanup) because the `DocumentStore` trait changes in 131 assume the engine layer is self-contained. Tasks 1-3 are ordered by complexity (create has the most logic to move). Task 4 is a verification step.
