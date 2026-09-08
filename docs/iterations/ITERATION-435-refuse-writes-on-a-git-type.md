---
title: Refuse writes on a git type
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- implements: STORY-281
- blocks: ITERATION-436
reviewed: bf44ec29495e6ef41d05c55f1385d7e9e8624a48
---

## Objective

Every mutating command on a `git` type exits non-zero naming the backend and saying writes are not yet supported, before any file under the clone is touched.

## Satisfies

STORY-281 AC6. AC5, AC9 deferred -- see Out of scope.

## Context

- Story + ACs: STORY-281 (Context paragraph two). Precedent: `UnavailableStore` at `src/engine/store_dispatch.rs:135-161`, the `ClickupTasks` fallback registration at `:3078-3088`, `for_type` at `:2951`.
- **One message, one place.** `pub(crate) fn git_write_refusal(type_def: &TypeDef) -> String` in `store_dispatch.rs`, spelled like `:3082-3085`: `type '<name>' uses git store; writes are not yet supported (STORY-282)`. `build_registry` (`:2972`) registers `UnavailableStore { message }` under `StoreBackend::Git` unconditionally.
- **Where each command writes today:**
  - `update` (`src/engine/ops/update.rs:130-150`) and `delete` (`src/engine/ops/delete.rs:27-46`): non-filesystem -> `registry.for_type(..)`. Free once registered.
  - `create` (`src/engine/ops/create.rs:85-212`): no `Git` branch, so it falls to `fs_ops::create_document` at `:194` and writes `<root>/<dir>` -- the silent local write AC6 forbids. Add before the `parent` branch at `:85`: `if type_def.store == Git { return build_registry(root, config).for_type(type_def)?.create(..) }`, so `--parent` is refused too.
  - `tag` (`src/cli/tag.rs:10-33`, `:35-56`): `rewrite_frontmatter` on the doc path first, `propagate_tags` (`:67-87`) via the registry after -- the cache file is mutated before the refusal. Move the `propagate_tags` call ahead of the rewrite in both functions: push-first, the policy `link` states at `src/engine/ops/link.rs:129-131`. Existing `tag.rs` tests at `:91+` cover filesystem, where the order is invisible.
  - `link`/`unlink` (`link.rs:37`, `:640`): `rewrite_frontmatter(&full_path, ..)` at `:140`/`:740` writes the source doc; `push_if_github_backed` covers GitHub only. Guard with `store_of(config, store, &from_id)` (`:583`) `== Some(Git)` -> `bail!(git_write_refusal(..))` before the native-edge calls at `:97`. Source only: the rewrite touches one file.
- **Registry test shape:** `build_registry_without_clickup_type_registers_unavailable` at `store_dispatch.rs:5671`.
- Fixture: ITERATION-434's `tests/integration/git_store_test.rs` `shared_repo()`; `cli::create::run` as used in `tests/integration/cli_create_test.rs:54`.

## Tasks

1. Test-first, `store_dispatch.rs` beside `:5671`: `build_registry` on a config with a `git` type -> `for_type(git_td)?.update(..)` errors containing `git` and `not yet supported`.
2. Add `git_write_refusal`, register `UnavailableStore` for `Git`. Green.
3. Test-first, `tests/integration/git_store_test.rs`: load a project whose `git` type has cloned (ITERATION-434 helper), then for `create::run`, `ops::update`, `ops::link::link_with_config`, `cli::tag::tag_add_with_config`, `ops::delete` -- each returns `Err` whose text contains `git` and `not yet supported`; the clone's doc file bytes are unchanged after all five; `<root>/docs/rfcs` does not exist after `create`.
4. `create.rs` branch, `tag.rs` reorder, `link.rs` guard. Green.
5. Confirm `clippy -D warnings`: the `Git` arm in `create` must not shadow the exhaustive-match sites from ITERATION-433.

## Out of scope

- AC5, AC9 -> ITERATION-436.
- TUI external-edit push (`src/tui/infra/event_loop.rs:120-235`) returns `Ok` for a `git` doc, leaving the cache edit local until the next fetch resets it. STORY-282 owns the write; ITERATION-436's `reset --hard` refresh is what reverts it.
- `create --parent` same-repo comparison (`create.rs:251`, RFC-072 "The git store" last paragraph) -> STORY-282, moot while `create` is refused.
- `fix --status` (`src/engine/ops/fix/status.rs:41`) already skips non-filesystem.
- Actual writes, commit, push -- STORY-282.

## Principles/conventions

`cargo run -q -- convention`. Principle 6: reuse `UnavailableStore`, no new store type. DICTUM-006: the error is the `anyhow` message a human reads, exit code from `main`. DICTUM-004: bytes-unchanged assertion on the clone, one TempDir per test.

## Verification

Scratch project from ITERATION-434's Verification: `cargo run -q -- create rfc "x" --json; echo $?` prints a JSON error naming `git` and `1`; `git -C .lazyspec/cache/<type> status --porcelain` is empty after `tag`, `link`, `update` attempts.
