---
title: Resolve every doc root against the extended root
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-09
tags: []
related:
- implements: STORY-284
- blocks: ITERATION-443
reviewed: 4fe4f87707975ac0780871beb31ecbc3a3973b4e
---

## Objective

Under `extends`, every `filesystem` type's documents and the templates directory resolve against the extended root while `governs`, staleness anchors, `@ref` expansion and `.lazyspec/cache/` stay on the local root; `config --json` `resolved_dir` shows the move.

## Satisfies

STORY-284 AC2, AC4, AC11. AC8-AC10, AC12, AC13 deferred -- see Out of scope.

## Context

- Story + ACs: STORY-284 (Context paragraph two). Contract: RFC-072 Design "The config override" table, Decision 4. Depends on ITERATION-441's `Config.extends`.
- **`cwd` never moves.** `Config::docs_root(&self, root: &Path) -> PathBuf` = `self.extends.as_ref().map_or(root.to_path_buf(), |e| e.root.clone())`. Everything below reads that; the 73 `&cwd` sites in `main.rs` are untouched.
- **`doc_root` gains the config** (`src/engine/store.rs:118-129`), matching RFC-072 Interfaces: `doc_root(config: &Config, root: &Path, type_def: &TypeDef)`. `Filesystem` -> `normalize(&config.docs_root(root).join(&type_def.dir))`; the cache arms keep `root` (Decision 4: cache is local); `Git` keeps `root` for the same reason. Callers, all with a `Config` in hand: `src/cli/config.rs:227`, `src/engine/git_store.rs:95`, `src/engine/sync.rs:246`, `store.rs:192`, `src/engine/validation.rs:1376` (`config` is in scope at `:1357`), tests `store.rs:1551-1594`, `tests/integration/git_store_test.rs:104`, `:107`, `:217`, `tests/integration/store_test.rs:1054`.
- **Four joins that bypass `doc_root`.** `fs_ops::create_document` joins `dir` onto `root` (`src/engine/fs_ops.rs:109`); both filesystem callers pass `&type_def.dir` -- `FilesystemStore::create` (`src/engine/store_dispatch.rs:493`) and `ops::create` (`src/engine/ops/create.rs:209`). Pass `doc_root(config, root, type_def).to_string_lossy()` instead: `Path::join` discards `root` for an absolute path, which is how STORY-283's out-of-root `dir` already creates. `ops/fix/conflicts.rs:93` `root.join(&type_def.dir)` -> `doc_root`. `engine/watch.rs:21` `root.join(&t.dir)` -> `doc_root` (the config-path half of `watch_paths` is ITERATION-445's). `src/cli/init.rs:82`, `:471` stay: `init` writes a fresh config, never an `extends` one.
- **Templates** (`fs_ops::load_template`, `src/engine/fs_ops.rs:20-21`): `config.docs_root(root).join(&config.filesystem.templates.dir)`.
- **The missing-dir warning** (`store.rs:207-217`) keys on `Path::new(&type_def.dir).is_absolute()`. Under `extends` a relative `dir` is still an external location, so the condition becomes `is_absolute() || config.extends.is_some()`; STORY-283 AC7's silent skip stays for a plain local relative dir.
- **What stays local, by anchor** -- pin, do not change: `governs_root` `store.rs:265` (`root.join(&config.governs.root)`); `src/cli/why.rs:45`; `validation.rs:985`; `src/engine/staleness.rs:173`; `RefExpander::new(root)` `src/tui/state/expansion.rs:70`; the cache root `store.rs:119`, `:195`; the `git` clone root `:195`.
- Fixtures: `store_test.rs:1024-1060` builds an out-of-root type; `tests/integration/extends_test.rs` (ITERATION-441) has the two-project layout; `git_store_test.rs:40` `shared_repo` for a `git` type inside an extended config.

## Tasks

1. Test-first, `store.rs` tests beside `:1551`: `doc_root` with `extends = Some(/shared)`: filesystem `docs/rfcs` -> `/shared/docs/rfcs`; `github-issues` -> `<root>/.lazyspec/cache/<name>`; `git` -> `<root>/.lazyspec/cache/<name>/<dir>`; `extends = None` -> today's answers.
2. `docs_root`, the signature, every caller. Green on the existing suites.
3. Test-first, `extends_test.rs`, project B extending A: (a) `Store::load(B, &config)` lists A's docs, each `path` absolute under `A/docs/<dir>`; `list <type> --json` and `show <ID> --json` through the binary agree (AC2); (b) `config --json` `.types[].resolved_dir` all start with A's root (AC11); (c) `store.governs_root() == B` and `why <file under B/src>` names the A doc whose `governs` covers it (AC4); (d) `create <type> "x" --json` in B writes under `A/docs/<dir>`, nothing under `B/docs`, and the template came from `A/.lazyspec/templates/template.md` (write a marker line there); (e) A's config carries a `git` type pointing at `shared_repo()`: after `list`, the clone is at `B/.lazyspec/cache/<type>`, nothing under `A/.lazyspec/cache` (AC4); (f) A's config names a type whose relative `dir` does not exist in A -> stderr `warning:` naming `A/<dir>`.
4. The four joins, templates, the warning. Green.
5. `clippy -D warnings`.

## Out of scope

- The extended `.lazyspec.toml` in the watch set and the TUI `FileChange` compare -> ITERATION-445 (AC12). Only the type-dir half of `watch_paths` changes here.
- URL `extends` -> ITERATION-443; `fetch` -> ITERATION-444; docs -> ITERATION-446.
- `[governs] root` in the *extended* config: it resolves against the local root like everything code-facing (Decision 4). No special case.
- `fix --renumber` / `fix --conflicts` end-to-end under `extends`; the join at `conflicts.rs:93` is corrected, not separately exercised.

## Principles/conventions

`cargo run -q -- convention`. Principle 6: one `docs_root` accessor; no `Roots` struct, no second `doc_root`. Principle 3: `cli` and `tui` never compute a doc path themselves. DICTUM-004: real files in TempDirs, one real git remote only for task 3(e).

## Verification

ITERATION-441's scratch dir extending this repo: `cargo run -q -- list rfc --json | jq -r '.[0].path'` is absolute under this repo's `docs/rfcs`; `cargo run -q -- config --json | jq -r '.types[0].resolved_dir'` likewise; `ls .lazyspec/cache` in the scratch dir after `cargo run -q -- list` shows only local cache dirs.
