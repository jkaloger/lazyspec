---
title: Declare store = git with remote and branch
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- implements: STORY-281
- blocks: ITERATION-434
reviewed: bf44ec29495e6ef41d05c55f1385d7e9e8624a48
---

## Objective

`store = "git"` parses, round-trips through the config writer, and `doc_root` resolves it to `<root>/.lazyspec/cache/<type>/<dir>` so `config --json` reports `resolved_dir` for the new backend.

## Satisfies

STORY-281 AC7. AC1-AC6, AC8, AC9 deferred -- see Out of scope.

## Context

- Story + ACs: STORY-281. Contract: RFC-072 Design "The git store" and "Resolution, not a second store", Decision 1, Interfaces.
- **Variant:** `StoreBackend` at `src/engine/config.rs:544`, `#[serde(rename = "git")] Git`; `Display` arm at `:560-571`; the `store` field doc at `:758-760` lists the backends.
- **Fields, not a struct.** `remote: Option<String>`, `branch: Option<String>` on `TypeDef` (`config.rs:734`), `#[serde(default, skip_serializing_if = "Option::is_none")]` like `clickup_custom_field_map` at `:847`. RFC drafts `GitStoreConfig`; two `Option`s on the type table are what `remote = "<url>"` in `[[types]]` deserialises to without `flatten`.
- **Validation in `parse_inner`** beside the `clickup_task_type` check at `config.rs:1989-1998`: `store = "git"` without `remote` is an error naming the type; `remote`/`branch` on any other store is an error in that check's shape; a `git` type whose `dir` is absolute is an error (join would discard the clone root).
- **Full `TypeDef` literals to extend** (everything else spreads `..test_fixture`): `config.rs:1497`, `TypeDef::test_fixture` `:2219`, `src/cli/config.rs:359` (`type_def_from_parts`), `src/tui/state/app.rs:3095-3112`.
- **Writer:** `update_type_table` in `src/engine/config_write.rs:75-107` hand-writes each key -- two `set_opt_str` lines.
- **`doc_root` arm** (`src/engine/store.rs:117-126`): `normalize(&root.join(".lazyspec/cache").join(&type_def.name).join(&type_def.dir))`. Stays `-> PathBuf`, no `config`, no `Result`: ensuring the clone is `load_with_fs`'s job (ITERATION-434), resolution is pure. ITERATION-430 said STORY-281 widens it; it does not need to.
- **Compiler-forced exhaustive arms:** `src/engine/sync.rs:387-395` (`false`) and `:400-443` (`None`, like Filesystem, until the fetch iteration), `src/engine/github_url.rs:164` (`None`). `store.rs:1514` asserts the five cache backends ignore `dir`; `Git` is not one of them.
- `run_show_json` (`src/cli/config.rs:212`) already writes `doc_root` into `resolved_dir`; nothing to change there.

## Tasks

1. Test-first, `config.rs` tests: `store = "git"` + `remote` parses to `StoreBackend::Git` with `remote`/`branch` populated; `git` without `remote` errors naming the type; `remote` on `store = "filesystem"` errors; absolute `dir` on `git` errors; `StoreBackend::Git.to_string() == "git"` beside `:3804`.
2. Add the variant, the two fields, the `Display` arm, the `parse_inner` checks. Extend the four literals. Green.
3. Test-first, `config_write.rs`: a `git` type with `remote` and `branch` survives `write_config_in_place` and re-parses equal. Add the two `set_opt_str` lines.
4. Test-first, `store.rs` beside `:1514`: `doc_root` on root `/a/b`, type `spec` with `dir = "docs/specs"`, store `Git` -> `/a/b/.lazyspec/cache/spec/docs/specs`. Add the arm; fill the `sync.rs` and `github_url.rs` arms.
5. Extend `show_json_emits_resolved_dir_beside_raw_dir_for_every_type` (`cli/config.rs:1317`) with one `git` type; expect the clone-joined path.

## Out of scope

- AC1-AC4, AC8 (clone on first read, gitignore guard, error text) -> ITERATION-434.
- AC6 (write refusal) -> ITERATION-435. AC5, AC9 (fetch, TUI parser and poll) -> ITERATION-436.
- `add-type --remote`/`--branch` flags, `--store` help text at `cli/config.rs:52`, `parse_store` `:1197`, TUI `store_from_variant`/`STORE_VARIANTS` -- AC9 and STORY-284.
- README -- STORY-284.

## Principles/conventions

`cargo run -q -- convention`. DICTUM-001: exhaustive `match`, no wildcard arms. Principle 6: two `Option<String>`, no `GitStoreConfig` struct for one consumer. DICTUM-004: fixed paths, no `TempDir` in the `doc_root` test.

## Verification

Scratch `[[types]]` with `store = "git"`, `remote = "/nonexistent.git"`, `dir = "docs/rfcs"`: `cargo run -q -- config --json | jq -r '.types[] | select(.store=="git") | .resolved_dir'` ends in `/.lazyspec/cache/<name>/docs/rfcs`. Remove `remote`: `config --json` exits non-zero naming the type.
