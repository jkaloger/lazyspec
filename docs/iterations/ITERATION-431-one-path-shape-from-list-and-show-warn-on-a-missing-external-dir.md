---
title: One path shape from list and show, warn on a missing external dir
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- implements: STORY-283
- blocks: ITERATION-432
reviewed: ead6302e2ac8d5f3cfc0277ede6907ec26112f68
---

## Objective

`list`/`show` report one path shape per type whatever the `dir` spelling, and a missing absolute external `dir` warns instead of reading as empty.

## Satisfies

STORY-283 AC3, AC5, AC6, AC7. AC8 deferred -- see Out of scope.

## Context

- Story + ACs: STORY-283. Contract: RFC-072 Design "Resolution, not a second store", second paragraph.
- **The shape bug is upstream of the loader.** `src/engine/store/loader.rs:75` `path.strip_prefix(root).unwrap_or(path)`, same idiom at `:140` (subdir index) and `:183` (virtual folder). With an unnormalised `full_path = root.join("../shared-specs")` the strip succeeds and `..` survives. ITERATION-430's `doc_root` normalises, so the strip fails and all three sites fall back to absolute. Expect no loader edit; prove it with the tests before touching the loader.
- **Absolute `meta.path` is safe downstream.** `reload_file` `store.rs:383` `root.join(relative_path)` -- join discards root on absolute. `extract_id` reads the filename. `cli/json.rs:38` prints `to_string_lossy`. `create` (`fs_ops.rs:109`) already works per STORY-283 Notes and is not `list`/`show`.
- **Warn condition:** `type_def.store == Filesystem && Path::new(&type_def.dir).is_absolute() && !fs.exists(&full_path)`, at the `continue` in `store.rs:136-144`. Relative missing keeps today's silent `continue` (AC7). The git-ref materialise branch is untouched.
- **Channel: a field on `Store`, not a signature change.** `Store::load` has 26 call sites in `src/main.rs`, ten in `src/tui/infra/event_loop.rs`, one in `src/web/server.rs:163`. Add `pub(crate) warnings: Vec<String>` beside `parse_errors` (`store.rs:40`) and `pub fn warnings(&self) -> &[String]`. Engine never prints (Principle 3). Two `Store {` test literals gain the field: `src/tui/state/app.rs:4423`, `:4774`.
- **CLI surface:** one `fn load_store(cwd, config) -> Result<Store>` in `main.rs` that `eprintln!("warning: {w}")` per entry (`cli/fetch.rs:203` is the spelling), replacing the 26 `Store::load(&cwd, &config)?`. stderr, so `--json` stdout stays parseable.
- **TUI surface:** `tui/views/overlays.rs:881` and `tui/views/status_bar.rs:280` already read `app.store.parse_errors()` off the store; read `app.store.warnings()` beside them. No new `App` field.
- Web: no warning surface exists. None added.

## Tasks

1. Test-first, `tests/integration/store_test.rs`: sibling `TempDir`s `project` and `shared`; one type spelled `dir = "../shared"`, the same type spelled with the absolute path; `Store::load` each; `list` paths equal, absolute, free of `..`. Repeat with `subdirectory = true` for `loader.rs:140`/`:183`.
2. Test-first, AC5 contract, same file: relative, absolute, escaping types -- every `list` path and every `store.get` path, `root.join(path)` `starts_with(doc_root(root, type_def))`.
3. Green: `load_with_fs` takes `full_path` from `doc_root`. Only if a `..` still leaks, normalise `full_path` once before `read_dir` -- nowhere else.
4. Test-first, `store.rs` `#[cfg(test)]` on `InMemoryFileSystem` (`store.rs:1044`): absolute missing `dir` -> one warning containing the resolved path, zero docs; relative missing `dir` -> no warning; cache-backed missing -> no warning.
5. `warnings` field + accessor; push in the `store.rs:142` branch. Fill `app.rs:4423`, `:4774`.
6. `main.rs` `load_store` wrapper; replace the 26 sites in one pass. `overlays.rs:881`, `status_bar.rs:280` read `warnings()`.
7. Integration, `tests/integration/cli_validate_test.rs`: absolute missing `dir` -- `validate` exit code unchanged, `run_json` output parses, no error row for it.
8. README "Store backends" (`:670`): external `filesystem` dirs list and show with absolute paths; a missing absolute `dir` warns on stderr; a missing relative `dir` is silent.

## Out of scope

- AC8 -> ITERATION-432.
- `create --json` path shape, `fs_ops.rs:109`.
- A `warnings` row on `validate --json` -- not a `ValidationIssue`; stderr only.
- Web warning surface. `git` store (STORY-281). `extends` (STORY-284).

## Principles/conventions

`cargo run -q -- convention`. Principle 3: the engine records the warning, CLI and TUI print it. Principle 6: a `Vec<String>`, not a warning type. DICTUM-004: `TempDir` per test, in-memory fs for the store unit tests.

## Verification

Scratch `[[types]]` with `dir = "/nonexistent/specs"`: `cargo run -q -- list --json 2>err.txt | jq . >/dev/null` succeeds and `err.txt` names `/nonexistent/specs`. Scratch type with `dir = "../<sibling holding one doc>"`: `list --json` path is absolute, no `..`, identical to the absolute spelling.
