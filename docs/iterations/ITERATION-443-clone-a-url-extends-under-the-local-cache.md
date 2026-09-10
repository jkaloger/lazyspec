---
title: Clone a URL extends under the local cache
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-09
tags: []
related:
- implements: STORY-284
- blocks: ITERATION-444
reviewed: 4fe4f87707975ac0780871beb31ecbc3a3973b4e
---

## Objective

`extends = "<url>[#<branch>]"` clones the remote under the *local* `.lazyspec/cache/config/` on first load and reads the config there; a later load reuses the clone without touching the network.

## Satisfies

STORY-284 AC8, AC10. AC9, AC12, AC13 deferred -- see Out of scope.

## Context

- Story + ACs: STORY-284 (Notes paragraph two, the ordering inversion). Contract: RFC-072 Design "The config override" ("a URL is resolved by the same clone machinery as the `git` store"). Depends on ITERATION-441's `probe`/`Extends`.
- **URL or dir.** In `src/engine/config/extends.rs`: `fn is_url(spec: &str) -> bool` = contains `://`, or has a `:` before its first `/` (scp `user@host:path`). Anything else is a directory (ITERATION-441). A bare local path to a repo is therefore a *dir*; tests that want clone semantics against a TempDir spell it `file://<path>`, which `git clone` accepts. Split the fragment on the last `#`: `(url, Some(branch))`.
- **`Config::load` needs git ops.** Today `load(project_root, fs)` (`src/engine/config.rs:2082`) has no `GitRefOps`. Mirror `Store::load`/`load_with_fs` (`src/engine/store.rs:151-161`): `pub fn load_with_git(project_root, fs, ops: &dyn GitRefOps) -> Result<Self>` does the work; `load` calls it with `&crate::engine::git_ref::GitCli`. `load_lenient` likewise. No `main.rs`, TUI or web call site changes (`src/main.rs:123`, `src/tui/infra/event_loop.rs:493`, `src/web/server.rs:148`).
- **The clone.** `clone_root = project_root.join(".lazyspec/cache/config")`. `!fs.exists(&clone_root)` -> `ensure_cache_gitignored(project_root, fs)` (`store.rs:133`, `pub(crate)`) then `ops.clone_repo(url, branch, &clone_root)` (`src/engine/git_ref.rs:35`; `GitCli` impl `:270-284`, `--single-branch`, `GIT_TERMINAL_PROMPT=0`) `.with_context(|| "cloning <url> (<branch|default branch>) for extends")` -- the `clone_git_store` wording at `store.rs:162-168`. Exists -> use it, no fetch (AC8 second half). Then `read <clone_root>/.lazyspec.toml`, the chain check, `parse`, exactly as the dir branch. `Extends` gains `pub remote: Option<String>, pub branch: Option<String>` for ITERATION-444; `root` is the clone root, which is what `config --json` `.extends` reports.
- **Type named `config`.** `.lazyspec/cache/config` collides with a `git`/cache type named `config`. `bail!` in `Config::load_with_git` when `extends` is a URL and the loaded config declares a type named `config`; one `if`, no renaming.
- **Mock.** `MockGitRefClient` (`git_ref.rs:559`, `test-support` feature) records `clone_repo` at `:825`; read what it does with `dest` before writing the unit test -- the test may need to create `<dest>/.lazyspec.toml` itself in place of a real clone.
- Fixtures: `tests/integration/git_store_test.rs:15` `git`, `:161` `git_stdout`; a shared repo here is `git init` + a full `.lazyspec.toml` + `docs/` committed on `main`, with a second branch carrying an extra type.

## Tasks

1. Test-first, `extends.rs` tests: `is_url` true for `https://h/o/r.git`, `git@github.com:o/r.git`, `file:///tmp/x`, `ssh://git@h/o/r`; false for `../shared`, `/abs/dir`, `shared`. Fragment: `https://h/r.git#next` -> `("https://h/r.git", Some("next"))`, no `#` -> `None`.
2. Test-first, `config.rs` tests with `MockGitRefClient`: `load_with_git` on `extends = "https://h/r.git#next"` -> one `clone_repo` call with `(https://h/r.git, Some("next"), <root>/.lazyspec/cache/config)`, `<root>/.lazyspec/.gitignore` contains `cache/`; a second `load_with_git` after the clone dir exists -> zero further calls; queued clone `Err` -> `Err` naming the url and `next`.
3. `is_url`, the fragment split, `load_with_git`, the `config`-type guard. Green.
4. Test-first, `tests/integration/extends_test.rs`, real git, project B with `extends = "file://<A>"`: (a) `lazyspec list <type> --json` clones to `B/.lazyspec/cache/config`, lists A's committed docs, `config --json` `.extends` is that clone path; (b) `extends = "file://<A>#next"` -> `config --json` shows the type only `next` declares; (c) after (a), rename A's directory, run `list` again -> exit 0, same docs (AC8: existing clone, no network).
5. `clippy -D warnings`.

## Out of scope

- Bringing the clone current -> ITERATION-444 (AC9). Until then a URL `extends` is frozen at clone time.
- Auth. RFC-072 Non-goals: whatever `git` does for the URL.
- A URL whose default branch has no `.lazyspec.toml`: the dir branch's "no `.lazyspec.toml` at <path>" error already fires on the clone root; no extra wording.
- Detecting a local directory that is also a git repo and cloning it; it is a dir (Decision 1's spirit: two spellings, one resolution).
- Windows drive letters in `is_url` (`C:\x` reads as scp). darwin/linux tool.

## Principles/conventions

`cargo run -q -- convention`. Principle 4 / DICTUM-002: the clone goes through `GitRefOps`; the unit tests never spawn git. Principle 6: `load_with_git` is the one seam, `load` is the `GitCli` default as `Store::load` is. DICTUM-006: the clone error names url and branch. Ponytail: `is_url` is a two-rule heuristic, marked with its ceiling.

## Verification

Scratch dir with `extends = "file:///Users/jkaloger/thezone/lazyspec"`: `cargo run -q -- list rfc --json | jq length` is non-zero, `ls .lazyspec/cache/config/.lazyspec.toml` exists, `cat .lazyspec/.gitignore` shows `cache/`. `extends = "file:///Users/jkaloger/thezone/lazyspec#main"` after `rm -rf .lazyspec/cache/config` clones `main`.
