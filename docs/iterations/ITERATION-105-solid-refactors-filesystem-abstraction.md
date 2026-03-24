---
title: SOLID Refactors - Filesystem Abstraction
type: iteration
status: accepted
author: unknown
date: 2026-03-23
tags: []
related:
- implements: docs/stories/STORY-084-solid-refactors.md
---



## Context

Implements stream 6d from STORY-084. The filesystem abstraction has the widest blast radius of the six streams: it touches most function signatures in both the engine and CLI layers. This iteration should run after the module splits (stream 5) are complete so the diff targets the new, smaller file structure rather than the original monolithic files.

## Changes

### 6d: FileSystem Trait

- [ ] Define `FileSystem` trait in a new `engine/fs.rs` module:
  - `read_to_string(&self, path: &Path) -> Result<String>`
  - `write(&self, path: &Path, contents: &str) -> Result<()>`
  - `rename(&self, from: &Path, to: &Path) -> Result<()>`
  - `read_dir(&self, path: &Path) -> Result<Vec<DirEntry>>`
- [ ] Implement `RealFileSystem` as the production implementation, delegating directly to `std::fs`
- [ ] Export `FileSystem` and `RealFileSystem` from the `engine` crate root
- [ ] Thread `&dyn FileSystem` (or a generic `F: FileSystem`) through `Store::load` and `Store::load_type_directory` signatures
- [ ] Thread the trait through all `cli/fix` entry points that call `std::fs` directly (`fields.rs`, `renumber.rs`, `conflicts.rs`)
- [ ] Replace all direct `std::fs::read_to_string`, `std::fs::write`, `std::fs::rename`, `std::fs::read_dir` call sites with the trait method calls
- [ ] Update `main.rs` and CLI entry points to construct `RealFileSystem` and pass it down
- [ ] Implement `MockFileSystem` in `#[cfg(test)]` scope with an in-memory map of `Path -> String`
- [ ] Write one demonstration test using `MockFileSystem` to load a minimal store without touching the real filesystem

## Test Plan

- [ ] Run `cargo test` before and after; no regressions
- [ ] The demonstration `MockFileSystem` test: construct two in-memory documents, call `Store::load` with the mock, assert the store contains both documents by ID
- [ ] Run `cargo run -- show STORY-084 --json` against the real docs directory to confirm `RealFileSystem` behaves identically to the previous `std::fs` calls
- [ ] Run `cargo run -- fix --dry-run` to confirm fix paths compile and produce the same output

## Notes

Threading a trait through signatures is mechanical but touches many files. Work top-down: start with `Store::load`, confirm it compiles, then propagate to callers. Use `cargo check` after each layer to catch missed sites.

The `MockFileSystem` can be minimal for this iteration: only the methods exercised by `Store::load` need to be implemented. Stub the rest with `unimplemented!()` behind a `cfg(test)` gate.

The `read_dir` return type (`Vec<DirEntry>`) may require a newtype wrapper since `std::fs::DirEntry` is not constructible in tests. Consider returning `Vec<PathBuf>` from the trait instead, which is trivially constructible and sufficient for the traversal logic.
