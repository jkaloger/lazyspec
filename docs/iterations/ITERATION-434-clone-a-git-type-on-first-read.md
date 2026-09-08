---
title: Clone a git type on first read
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- implements: STORY-281
- blocks: ITERATION-435
reviewed: bf44ec29495e6ef41d05c55f1385d7e9e8624a48
---

## Objective

A `git` type's first read clones its remote under `.lazyspec/cache/<type>/`, checks out `branch` or the remote default, gitignores the cache, and reads from the existing clone thereafter without touching the network.

## Satisfies

STORY-281 AC1, AC2, AC3, AC4, AC8. AC5, AC6, AC9 deferred -- see Out of scope.

## Context

- Story + ACs: STORY-281. Contract: RFC-072 Design "The git store" (first two paragraphs).
- **The seam already reaches `load_with_fs`.** `Store::load_with_fs` (`src/engine/store.rs:134-139`) takes `git_ref_ops: Option<&dyn GitRefOps>` and `Store::load` (`:129`) passes `GitCli`. The story's "eleven signature sites" is void: add the op to the trait, thread nothing. `tests/integration/cli_git_ref_show_test.rs:28` and `src/tui/state/settings_guard.rs:149` pass `None` and stay untouched.
- **One new op on `GitRefOps`** (`src/engine/git_ref.rs:13-66`): `fn clone_repo(&self, remote: &str, branch: Option<&str>, dest: &Path) -> Result<()>`. `GitCli` (`:106`): `git clone --single-branch [--branch <b>] <remote> <dest>` built like `fetch_refs` at `:239-255` -- `GIT_TERMINAL_PROMPT=0`, `subprocess::output_with_timeout`, `bail!` with trimmed stderr on failure. Three impls to extend: `GitCli`, `MockGitRefClient` (`:638`, a `clone_results` queue plus a `clone_repo:<remote>:<branch>:<dest>` entry in `calls`, `pop_or_default` at `:628`), `RenamingGit` (`tests/integration/cli_fix_governs_test.rs:35`, `unreachable!`).
- **The branch in `load_with_fs`** (`store.rs:147-176`): for `StoreBackend::Git`, `clone_root = root.join(".lazyspec/cache").join(&type_def.name)`; when `!fs.exists(&clone_root)` and `ops` is `Some`, `ensure_cache_gitignored(root, fs)?` then `ops.clone_repo(remote, branch, &clone_root)`. `remote` is `Some` by ITERATION-433's parse check; `expect` with that reason (DICTUM-001). Then fall through to the existing `full_path` handling, which is `doc_root` (the `<clone>/<dir>` join, AC1). When the clone exists no op is called (AC3).
- **Error text (AC8):** `.with_context(|| format!("cloning {remote} ({branch}) for type {name}"))` with `branch.unwrap_or("default branch")`. git's own stderr (unreachable host, unknown branch) is the cause underneath.
- **The gitignore guard** (`src/engine/git_ref_store.rs:16-34`) is private, `std::fs`, one caller at `:287`. Move it to `store.rs` beside `doc_root` as `pub(crate) fn ensure_cache_gitignored(root: &Path, fs: &dyn FileSystem) -> Result<()>` -- `FileSystem` (`src/engine/fs.rs:24-32`) has `read_to_string`/`write`/`create_dir_all`/`exists`; rewrite the whole file instead of appending. `GitRefStore::create` calls it with `&RealFileSystem`.
- **Tests:** `InMemoryFileSystem` (`store.rs:1048`) + `MockGitRefClient` for the branch logic; a real local repo for the clone itself. `TestFixture::with_git_remote` (`tests/integration/common/mod.rs:144`) shows the git-init idiom; the git-store remote is a plain `git init` TempDir with docs committed on `main` and a second branch, its path as `remote`. `git clone <path>` needs no network.

## Tasks

1. Add `clone_repo` to the trait, `GitCli`, `MockGitRefClient`, `RenamingGit`.
2. Test-first, `store.rs` `#[cfg(test)]` beside `:1773`: (a) `git` type, no clone dir -> `calls` holds one `clone_repo` entry with the remote, `Some(branch)` and `/fake/root/.lazyspec/cache/<name>`; (b) same with `branch: None` -> entry says default; (c) clone dir present with a doc at `<clone>/<dir>/X-001-a.md` -> zero `clone_repo` calls, one doc listed, even with a queued `Err` clone result; (d) queued `Err("could not read from remote")` and no clone dir -> `load_with_fs` errors, message contains the remote and the branch; (e) after (a), `fs` holds `/fake/root/.lazyspec/.gitignore` containing `cache/`; (f) `git_ref_ops: None` -> no call, no error, no docs.
3. Move `ensure_cache_gitignored`; add the `Git` branch. Green.
4. Integration, new `tests/integration/git_store_test.rs`: helper `shared_repo()` -> `TempDir` with `git init -b main`, user config, `docs/rfcs/RFC-001-a.md` committed, branch `next` adding `RFC-002-b.md`. (a) `Store::load` on a project with `store = "git"`, `remote = <path>`, `dir = "docs/rfcs"` -> `.lazyspec/cache/rfc/docs/rfcs/RFC-001-a.md` exists, `list` returns RFC-001 only, path under `doc_root`; (b) `branch = "next"` -> RFC-002 present; (c) after (a), delete the shared repo, `Store::load` again -> RFC-001 still listed, no error; (d) `.lazyspec/.gitignore` contains `cache/`; (e) `remote` pointing at an empty TempDir -> error mentions the path and `default branch`; (f) `branch = "nope"` -> error mentions `nope`.
5. Assert `git_ref_test.rs` create tests still pass with the moved guard.

## Out of scope

- AC5 (`fetch`, refresh of an existing clone) and AC9 -> ITERATION-436. A stale clone stays stale until then.
- AC6 (write refusal) -> ITERATION-435. Until it lands, `create`/`link`/`tag` on a `git` type write locally; do not paper over it here.
- A warning when the clone's `dir` does not exist -- `store.rs:158` warns only for absolute `filesystem` dirs; leave it.
- Shallow clones, timeouts beyond `output_with_timeout`, credentials (story Out of scope).

## Principles/conventions

`cargo run -q -- convention`. Principle 4 / DICTUM-002: the clone is a `GitRefOps` method, the fake lives in `test_support`; `FileSystem` for every file the engine touches. DICTUM-004: no network -- a TempDir repo is the remote; unit tests on the in-memory fs. Principle 3: engine clones, nothing prints.

## Verification

Scratch project with `store = "git"`, `remote = "$(pwd)"` (this repo), `dir = "docs/rfcs"`: first `cargo run -q -- list --json` populates `.lazyspec/cache/<type>/docs/rfcs/` and lists RFC-072; `cat .lazyspec/.gitignore` shows `cache/`; second run makes no git call (`GIT_TRACE=1` shows none).
