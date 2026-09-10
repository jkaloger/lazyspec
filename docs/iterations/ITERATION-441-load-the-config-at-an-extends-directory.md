---
title: Load the config at an extends directory
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-09
tags: []
related:
- implements: STORY-284
- blocks: ITERATION-442
reviewed: 4fe4f87707975ac0780871beb31ecbc3a3973b4e
---

## Objective

A `.lazyspec.toml` whose only key is `extends = "<dir>"` loads the config at that directory; a sibling key or a nested `extends` is a load error; `Config` carries the resolved root and `config --json` reports it.

## Satisfies

STORY-284 AC1, AC3, AC5, AC6, AC7. One parse function owns all five, so they ship together. AC2, AC4, AC8-AC13 deferred -- see Out of scope.

## Context

- Story + ACs: STORY-284. Contract: RFC-072 Design "The config override", Decision 2 (exclusive), Non-goals (no recursion).
- **Probe before `RawConfig`.** `parse_inner` (`src/engine/config.rs:1817`) deserializes into `RawConfig` (`:1428`) and bails on missing `[[types]]` (`:1824`) and `[[relationships]]` (`:1832`) before anything else could see `extends`. New `src/engine/config/extends.rs` (config.rs is 2.3k lines; file-as-module, `mod extends;` beside `mod tests` at `:2293`): `pub(crate) fn probe(toml_str: &str) -> Result<Option<String>>`. Parse with `toml_edit::DocumentMut` (already a dep, `Cargo.toml:30`; `toml` 0.8 has no `preserve_order`, so `toml::Table` cannot give file order). Iterate `doc.as_table().iter()`: no `extends` -> `Ok(None)`; `extends` plus others -> `bail!` naming the others comma-joined in file order (AC5); `extends` alone, non-string value -> `bail!`; else `Ok(Some(value))`.
- **`parse` refuses a one-liner; `load` resolves it.** `parse_inner` calls `probe` first: `Some(_)` -> `bail!("this config declares `extends`; load it through Config::load")`. That single line is what makes every config mutator refuse under `extends` -- `config add-type`, `set-edge`, the TUI settings save and `fix --config` all read back the bytes they are about to write through `Config::parse` (README "every mutator reads back the exact bytes"), and an appended `[[types]]` beside `extends` trips the exclusivity error before any write. No mutator code changes.
- **`Config::load`** (`:2082-2095`): after the exists check, `content` -> `probe`. `None` -> today's `parse`. `Some(spec)` -> `resolve_dir(project_root, &spec)`: absolute used as-is, relative joined onto `project_root` (the directory containing the file, AC7), then `normalize` (`src/engine/store.rs:69`, private -- make it `pub(crate)`); a missing directory or a directory with no `.lazyspec.toml` -> `bail!` naming the resolved path. Read `<resolved>/.lazyspec.toml`; `probe` again, `Some(_)` -> `bail!("extends chain: <resolved> itself declares extends")` (AC6); `parse` it; set `config.extends`. `load_lenient` (`:2097-2113`) shares the read: extract `fn read_source(project_root, fs) -> Result<(String, Option<Extends>)>` used by both so `fix --config` sees the same error rather than a lenient parse of the one-liner.
- **The field.** `pub struct Extends { pub root: PathBuf }` in `extends.rs`, re-exported from `config`. On `Config` (`:1116`): `#[serde(skip)] pub extends: Option<Extends>` -- skipped so `to_toml` never writes it and `config --json` does not double-report it. `Default` (`:1705`) -> `None`; struct literals without `..Default::default()` gain the field (RFC-069's `staleness` at `:1158` was the last such addition; follow its trail). `RawConfig` gains `extends: Option<String>` with a doc comment so `config schema` documents the key; the probe reads the file, so mark the field as schema-only the way `rules` (`:1442-1445`) is.
- **`config --json`** (`src/cli/config.rs:212-232`): insert `"extends": <root>` beside the injected `resolved_dir` when `Some`. AC1 is free: the `types`/`relationships`/`edges` serialized are the extended config's.
- **Test shapes.** `tests/integration/config_test.rs:82` for a parse error; `tests/integration/cli_no_config_test.rs:11-28` for driving the binary with `env!("CARGO_BIN_EXE_lazyspec")`; `tests/integration/common/mod.rs` `TestFixture` (`new` `:16`, `root` `:30`, `write_rfc` `:74`) for a populated shared project.

## Tasks

1. Test-first, `extends.rs` `#[cfg(test)]`: (a) no `extends` -> `None`; (b) `extends = "../x"` alone -> `Some("../x")`; (c) `extends` after `[[types]]` and before `[naming]` -> `Err` whose text lists `types, naming` in that order; (d) `extends = 3` -> `Err`.
2. `probe`, `Extends`, the `Config` field, `RawConfig` field. Green.
3. Test-first, `config_test.rs`: (a) `Config::parse` of a one-liner -> `Err` naming `Config::load`; (b) `Config::load` on a TempDir with `extends = "shared"` and `shared/.lazyspec.toml` holding a two-type config -> `types.len() == 2`, `extends.root == <tmp>/shared` normalized, no "missing required [[types]]"; (c) `extends = "shared"` where `shared/.lazyspec.toml` is `extends = "../other"` -> `Err` containing `chain`; (d) `extends = "nope"` -> `Err` naming `<tmp>/nope`; (e) `load_lenient` on the one-liner behaves as (b).
4. `Config::load`/`load_lenient` branch; `run_show_json` field. Green.
5. Test-first, new `tests/integration/extends_test.rs`: project B `.lazyspec.toml` = `extends = "../A"`, A a `TestFixture`-shaped project with a custom type. Through the binary in B: `config --json` `.types[].name` equals A's, `.extends` is A's absolute root, `.edges` is A's; `config add-type spike spikes docs/spikes SPIKE --json` exits non-zero, stderr names `types`, B's `.lazyspec.toml` bytes unchanged.
6. `clippy -D warnings`.

## Out of scope

- Any doc-root move: `list`/`show` in B still read B's `docs/` until ITERATION-442 (AC2, AC4, AC11).
- URL `extends`, the `#branch` fragment, the config clone -> ITERATION-443 (AC8, AC10). A value that is not a directory is an error here; ITERATION-443 teaches `load` to tell a URL from a path.
- `fetch` for the config clone -> ITERATION-444 (AC9). TUI/web reload -> ITERATION-445 (AC12). Docs -> ITERATION-446 (AC13).
- `deny_unknown_fields` on `RawConfig`. Story Notes: the exclusivity error is its own check, not a tightening.
- A friendlier `fix --config` under `extends`; it refuses through the same error and that is enough.

## Principles/conventions

`cargo run -q -- convention`. Principle 3: resolution is engine code; `main.rs:123` is untouched. Principle 6: one probe function, one `Extends` struct with one field; ITERATION-443 adds fields when it has a use for them. DICTUM-006: every error names the path the user wrote or the keys they must delete. Module Structure: new file, not another 100 lines in `config.rs`.

## Verification

Scratch dir with `extends = "../lazyspec"` (this repo): `cargo run -q -- config --json | jq '.extends, [.types[].name]'` prints this repo's absolute path and its type names. Append `[naming]\npattern = "x"` and rerun: exit 1, stderr lists `naming`.
