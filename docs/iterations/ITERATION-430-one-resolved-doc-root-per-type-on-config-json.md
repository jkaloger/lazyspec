---
title: One resolved doc root per type on config --json
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- implements: STORY-283
- blocks: ITERATION-431
reviewed: ead6302e2ac8d5f3cfc0277ede6907ec26112f68
---

## Objective

One `doc_root` function answers where a type's documents live, and `config --json` reports it per type as `resolved_dir`.

## Satisfies

STORY-283 AC1, AC2, AC4. AC3, AC5, AC6, AC7, AC8 deferred -- see Out of scope.

## Context

- Story + ACs: STORY-283. Contract: RFC-072 Design "Resolution, not a second store", Decision 3.
- **Resolution lives inline today.** `src/engine/store.rs:128-133`, a `match` inside `Store::load_with_fs` with a `_ =>` arm for filesystem. `normalize` at `store.rs:65` is private and I/O-free -- the filesystem arm gets it so `../shared-specs` comes back without `..`.
- **`root` is already absolute.** `src/main.rs:23` `std::env::current_dir()`. `Path::join` discards `root` on an absolute `dir`, so `normalize(root.join(dir))` is absolute for every spelling.
- **Signature: `pub fn doc_root(root: &Path, type_def: &TypeDef) -> PathBuf` in `store.rs`.** RFC-072 Interfaces drafts `(config, root, type_def) -> Result<PathBuf>`; the `config` and the `Result` exist for the `git` clone. STORY-281 widens it. Do not pre-widen.
- **Cache arm is byte-identical.** `root.join(".lazyspec/cache").join(&type_def.name)` for the five cache-backed stores; `dir` ignored. Exhaustive `match` on `StoreBackend` (`engine/config.rs:544`), no wildcard -- DICTUM-001.
- **`resolved_dir` is `Value` surgery, not a `TypeDef` field.** `run_show_json` (`src/cli/config.rs:207`) already mutates the `Value` for `edges`; do the same on each `types[i]`. `TypeDef` (`engine/config.rs:734`) is `Serialize + Deserialize` for the TOML round-trip; a field there lands in `.lazyspec.toml` via every writer.
- Callers to thread `root` through: `src/main.rs:689` (has `cwd`), tests `src/cli/config.rs:1254`, `:1373-1374`.

## Tasks

1. Test-first, `store.rs` `#[cfg(test)]`: `doc_root` on root `/a/b` -- `docs/rfcs` -> `/a/b/docs/rfcs`; `/tmp/x/specs` -> itself; `../shared-specs` -> `/a/shared-specs`; each of the five cache backends -> `/a/b/.lazyspec/cache/<name>` with a nonsense `dir`. Fixed paths, no `TempDir` (DICTUM-004 deterministic).
2. Extract `pub fn doc_root` from `store.rs:128-133`. Exhaustive match, `normalize` on the filesystem arm. `load_with_fs` calls it; nothing else in `load_with_fs` moves.
3. Test-first, `cli/config.rs` tests beside `:1254`: `run_show_json(root, &config)` gives every `types[i]` a `resolved_dir` equal to `doc_root`, absolute, with raw `dir` still present. One type per spelling plus one `github-issues`.
4. Add `root: &Path` to `run_show_json`; inject `resolved_dir` per type. Update `main.rs:689`, `config.rs:1254`, `:1373-1374`.
5. Integration, `tests/integration/config_test.rs`: `TempDir` root, three `filesystem` spellings and one cache-backed type, assert `resolved_dir` through `run_show_json`.
6. README `:622` block: one sentence -- types on `config --json` carry `resolved_dir`, absolute; cache-backed stores report the cache path and ignore `dir`.

## Out of scope

- AC3, AC5, AC6, AC7 -> ITERATION-431. `load_with_fs` changes only by calling `doc_root`; `store/loader.rs` untouched here.
- AC8 -> ITERATION-432.
- `git` arm, `Result` return, `config` parameter -> STORY-281. `extends` -> STORY-284.
- `TypeDef` fields, `config add-type` / `set-type` writers, TUI settings screen.

## Principles/conventions

`cargo run -q -- convention`. Principle 3: resolution is engine (`store.rs`); the CLI serialises what the engine returns. Principle 6: one function, no trait, no struct. DICTUM-001: exhaustive match. DICTUM-004: state-free unit tests with fixed paths.

## Verification

`cargo run -q -- config --json | jq -r '.types[] | "\(.dir) -> \(.resolved_dir)"'` on this repo: every right-hand side starts with `$PWD`, none contains `..`.
