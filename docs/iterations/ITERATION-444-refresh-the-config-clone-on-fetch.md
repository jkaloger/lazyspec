---
title: Refresh the config clone on fetch
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-09
tags: []
related:
- implements: STORY-284
- blocks: ITERATION-445
reviewed: 4fe4f87707975ac0780871beb31ecbc3a3973b4e
---

## Objective

`lazyspec fetch` brings a URL `extends` config clone to the remote's tip before it fetches any type, so the next command sees the new types and documents.

## Satisfies

STORY-284 AC9. AC12, AC13 deferred -- see Out of scope.

## Context

- Story + ACs: STORY-284. Contract: RFC-072 Goals ("`lazyspec fetch` brings that clone current"), Risks ("a managed clone can be stale ... `fetch` is the existing remedy"). Depends on ITERATION-443's `Extends { remote, branch }`.
- **One call at the top of `fetch::run`** (`src/cli/fetch.rs:22-31`; `git_ref_ops` is already a parameter, `main.rs:151-160` passes `&GitCli`). Before the type lists at `:32`: `if let Some(Extends { remote: Some(url), branch, .. }) = &config.extends` -> `git_ref_ops.update_clone(&root.join(".lazyspec/cache/config"), branch.as_deref())` (`src/engine/git_ref.rs:39`; `GitCli` `:286`, fetch + `reset --hard FETCH_HEAD`) `.with_context(|| "updating <url> (<branch|default branch>) for extends")` -- the `sync_git_clone` wording at `src/engine/sync.rs:250-256`. A failure is `Err` out of `run`: exit non-zero, nothing else fetched, matching how a missing GitHub repo or ClickUp token is a hard error at `:116-133` rather than a per-type outcome.
- **Do not print "no fetchable types"** when the config clone was refreshed. The early return at `:72-83` gains `&& !refreshed_extends`; human mode prints `Fetched extends from <url>` in place of it, `--json` prints an empty outcomes array as today's per-type loop would with nothing to do. No new JSON shape: `SyncOutcome` is per type and `extends` is not one.
- **This run still uses the old config.** `config` was loaded from the pre-fetch clone; the new types show on the *next* command, which is what AC9 says. Do not reload inside `fetch`.
- **TUI background poll** (`poll_sync`, `src/tui/infra/event_loop.rs:321`) mirrors `fetch` per type but does not refresh the config clone. Out of scope -- see below.
- Fixtures: ITERATION-443's `extends_test.rs` shared repo; `git_store_test.rs:175` `commit_count`; "commit straight into the remote" (`shared_repo`'s idiom, `:40-63`) moves A ahead; `lazyspec()` (`:578`) drives the binary.

## Tasks

1. Test-first, `src/cli/fetch.rs` `#[cfg(test)]` (or beside the existing fetch tests if any) with `MockGitRefClient`: `run` with `config.extends = Some(Extends { remote: Some(url), branch: Some("next"), root })` and no fetchable types -> one `update_clone(<root>/.lazyspec/cache/config, Some("next"))` call, `Ok`; queued `update_clone` `Err` -> `Err` naming the url and `next`; `extends` a dir (`remote: None`) -> zero calls and today's "no fetchable types" output.
2. The call, the context, the early-return guard. Green.
3. Test-first, `extends_test.rs`, project B with `extends = "file://<A>"`: `list` (clones); commit a new doc *and* a new `[[types]]` block into A; `lazyspec fetch --json` in B exits 0; `config --json` now lists the new type; `list <newtype> --json` shows the new doc; `git -C B/.lazyspec/cache/config rev-parse HEAD` equals A's `HEAD`. Then rename A away and `fetch` -> exit non-zero, stderr names `file://<A>`.
4. `clippy -D warnings`; README `fetch` paragraph is ITERATION-446's.

## Out of scope

- The TUI poll refreshing the config clone, and a `CacheRefresh` that re-reads config. AC12 covers an *edit* to the extended config; for a URL `extends` that edit arrives via `fetch`.
- `fetch --type <name>` targeting the config clone; there is no type to name.
- A `{"type": "extends"}` outcome entry; nothing consumes one.
- Refreshing the config clone in any command other than `fetch`. Risks: stale clone accepted.
- AC12 -> ITERATION-445. AC13 -> ITERATION-446.

## Principles/conventions

`cargo run -q -- convention`. Principle 3: the refresh is `GitRefOps::update_clone`, engine code the CLI calls. Principle 6: no `ConfigSync` implementing `TypeSync`; one call, one context string. DICTUM-004: unit test on the mock, one real git test. DICTUM-006: exit code carries the failure; stderr names the remote.

## Verification

ITERATION-443's scratch dir on `file:///Users/jkaloger/thezone/lazyspec`: commit a new doc in this repo, `cargo run -q -- fetch --json` in the scratch dir, then `cargo run -q -- list rfc --json | jq length` grew by one and `git -C .lazyspec/cache/config log -1 --oneline` matches this repo's `HEAD`.
