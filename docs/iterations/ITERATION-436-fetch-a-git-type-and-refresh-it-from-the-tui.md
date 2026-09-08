---
title: Fetch a git type and refresh it from the TUI
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- implements: STORY-281
reviewed: bf44ec29495e6ef41d05c55f1385d7e9e8624a48
---

## Objective

`fetch` brings a `git` type's clone current and reports it beside the other backends; the TUI offers `git` in the store field and refreshes it in the background poll.

## Satisfies

STORY-281 AC5, AC9.

## Context

- Story + ACs: STORY-281 (Notes: two store parsers). Contract: RFC-072 Design "The git store" ("brought current by `fetch`").
- **Second `GitRefOps` op** (`src/engine/git_ref.rs:13-66`): `fn update_clone(&self, clone: &Path, branch: Option<&str>) -> Result<()>` -- `git fetch origin <branch|HEAD>` then `git reset --hard FETCH_HEAD`, same `GIT_TERMINAL_PROMPT=0` / `output_with_timeout` shape as ITERATION-434's `clone_repo`. `reset --hard`, not `pull --ff-only`: a TUI external edit (ITERATION-435 Out of scope) leaves the working tree dirty and `pull` would refuse. Same three impls: `GitCli`, `MockGitRefClient` (`update_clone_results` queue, `calls` entry), `RenamingGit` (`unreachable!`).
- **Syncer** in `src/engine/sync.rs` beside `GitRefSync` (`:189-213`): `pub struct GitSync<'c> { pub ops: &'c dyn GitRefOps }`, `TypeSync::sync` -> if the clone root (`root/.lazyspec/cache/<name>`) is missing, `clone_repo` (ITERATION-434 op) else `update_clone`; the doc set under `doc_root(root, td)` before and after (a private recursive `.md` lister, ~10 lines; `fetched` = after count, `new`/`removed` = set differences); `Err` -> `SyncOutcome::failed` with the ITERATION-434 context text naming remote and branch. `Syncers` (`:292-298`) gains `pub git: Option<GitSync<'c>>`; `order` (`:317-322`) gains `Git` after `GitRef`; the `dispatch` `Git` arm (`:400-443`, `None` since ITERATION-433) becomes `run_syncer(syncers.git.as_mut(), .., "git")`.
- **CLI** `src/cli/fetch.rs:32-97`: a `git_types` list beside the four, in the empty check (`:64-68`), the `--type` check (`:78-82`) and its message (`:84`), a `fetch_git` filter, `syncers.git = Some(GitSync { ops: git_ref_ops })` beside `:177-182`. `outcomes_json` (`:255`) already emits `{type, fetched, new, removed}` per outcome.
- **TUI poll** `src/tui/infra/event_loop.rs`: `has_git` beside `:311`, `syncers.git` beside `:371-376`, and the spawn gate at `:947-952` (`shared_gh_store.is_some() || has_clickup_types`) gains `|| has_git_types`. `git-ref` is absent from that gate today; leave it.
- **TUI store parser** `src/tui/state/app.rs:156-165` `store_from_variant`: `"git"` arm. `STORE_VARIANTS` `src/tui/views/panels.rs:2092-2098`: add `"git"` so the field cycles to it. CLI parser `src/cli/config.rs:1197-1207` `parse_store`: `"git"` arm.
- Fixtures: ITERATION-434's `tests/integration/git_store_test.rs` `shared_repo()`; `poll_sync` test shape at `event_loop.rs:1389-1451`; `fetch::run` driven from `tests/integration/fetch_prune_test.rs`.

## Tasks

1. Add `update_clone` to the trait and three impls.
2. Test-first, `sync.rs` `#[cfg(test)]` with `MockGitRefClient` + TempDir: (a) no clone dir -> one `clone_repo` call, zero `update_clone`; (b) clone dir present -> one `update_clone` with `Some(branch)`, zero `clone_repo`; (c) queued `Err` -> `outcome.error` names remote and branch, `fetched == 0`. `GitSync`, `Syncers.git`, order, dispatch arm. Green.
3. Test-first, `tests/integration/git_store_test.rs`: (a) load once, commit `RFC-002-b.md` to the shared repo, `fetch::run(.., &GitCli, .., None, true)` on a config whose only remote type is `git` -> stdout parses, one entry `{type, fetched: 2, new: 1, removed: 0}`, no `no fetchable types`; `Store::load` lists RFC-002; (b) remove `RFC-001` upstream, fetch -> `removed: 1`, cache file gone; (c) `fetch --type <git-type>` runs it, `--type <filesystem-type>` errors with the `:84` message naming `git`. `fetch.rs` edits. Green.
4. Test-first, `event_loop.rs` beside `:1426`: `poll_sync` on a config with one `git` type and `MockGitRefClient` -> one `update_clone` call (clone dir pre-created), no warning. `has_git`, `syncers.git`, spawn gate. Green.
5. Test-first: `store_from_variant("git") == Some(Git)` beside the app.rs settings tests; `parse_store("git")` beside `cli/config.rs` tests; `STORE_VARIANTS.contains(&"git")`. Three one-line arms. Green.

## Out of scope

- `clickup-tasks` missing from `STORE_VARIANTS` -- pre-existing, not this story.
- `git-ref` in the poll spawn gate (`:952`) -- pre-existing.
- TUI fields for `remote`/`branch` in the settings pane; cycling to `git` without a `remote` fails ITERATION-433's parse check on save, which is the right message.
- `--help` and README wording of the `git`/`git-ref` split -- STORY-284.
- Writes -- STORY-282.

## Principles/conventions

`cargo run -q -- convention`. DICTUM-007: the poll thread sends `AppEvent::CacheRefresh`, mutates no state. Principle 4: `GitRefOps` fake at the seam, real `git` only against a TempDir repo. Principle 6: counts by set difference, no diff parsing. DICTUM-001: `dispatch` and `is_github_backend` stay exhaustive.

## Verification

Scratch project from ITERATION-434: commit a new RFC to the remote, `cargo run -q -- fetch --json | jq '.[] | select(.type=="<git-type>")'` shows `new: 1`; `cargo run -q -- list --json` includes it. `cargo run -q -- fetch --type rfc` on this repo still errors naming the four-plus-one backends.
