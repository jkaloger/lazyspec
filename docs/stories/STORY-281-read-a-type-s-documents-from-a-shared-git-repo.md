---
title: Read a type's documents from a shared git repo
type: story
status: complete
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- implements: RFC-072
- blocks: STORY-282
- blocks: STORY-284
reviewed: bf44ec29495e6ef41d05c55f1385d7e9e8624a48
---

## Context

A team running several services against one set of RFCs has to duplicate the docs into each repo or give up lazyspec in all but one. `git-ref` puts documents under refs of *this* repo; nothing puts them in another repo's worktree. A type declaring `store = "git"` with a clone URL closes that gap.

Read before write, deliberately. The codebase already ships this split: `ClickupTasksStore` registered read-first with writes refused explicitly (`src/engine/store_dispatch.rs:3050-3086`, `UnavailableStore` at `:135`). A service repo that consumes RFCs it never authors is the common case, so the read path is worth shipping alone -- provided the write refusal is explicit rather than a fall-through.

As a developer in a service repo, I want a type to declare `store = "git"` with a remote, so that I can read and validate the team's shared specs without those documents living in my repo.

## Acceptance Criteria

- **Given** a type with `store = "git"` and `remote = "<url>"`
  **When** I first run a command that reads that type
  **Then** lazyspec clones the remote under `.lazyspec/cache/<type>/` and lists the documents at `<clone>/<dir>` -- the type's `dir` joined against the clone root, not the clone root itself.

- **Given** `branch` is set on the type
  **When** the clone is created
  **Then** that branch is checked out; **given** `branch` is absent, **then** the remote's default branch is.

- **Given** the clone already exists
  **When** I read that type with the remote unreachable
  **Then** the read succeeds from the existing clone and no re-clone occurs.

- **Given** a clone is created for a `git` type
  **When** it lands
  **Then** `.lazyspec/.gitignore` contains `cache/` -- the clone path invokes the same guard `GitRefStore::create` calls at `git_ref_store.rs:287`, which today is private and runs only on a git-ref write.

- **Given** a `git` type
  **When** I run `lazyspec fetch` or `lazyspec fetch --type <git-type>`
  **Then** the clone is brought current, subsequent reads see the new documents, `--json` emits that type's `{type, fetched, new, removed}` entry alongside the other backends, and a project whose only remote type is `git` no longer reports "no fetchable types configured" (`src/cli/fetch.rs:64-97`).

- **Given** a `git` type
  **When** I run `create`, `update`, `link`, `tag` or `delete` on it
  **Then** the command exits non-zero naming the backend and saying writes are not yet supported -- the shape `ClickupTasksStore` uses at `store_dispatch.rs:3080`, not a silent local-only write.

- **Given** a `git` type
  **When** I run `config --json`
  **Then** `resolved_dir` is the type's `dir` joined against the managed clone root, extending STORY-283's contract to the new backend.

- **Given** an unreachable remote, a missing branch, or a failed clone
  **When** a read runs
  **Then** the error names the remote and the branch.

- **Given** a `git` type
  **When** I set its store from the TUI settings pane
  **Then** `git` is offered and accepted by the TUI's store parser at `src/tui/state/app.rs:162` as it is by the CLI's at `src/cli/config.rs:1185`, and the TUI's background refresh (`src/tui/infra/event_loop.rs:311`) treats a `git` type as fetchable, as it does `git-ref` and `clickup-tasks`.

## Scope

### In Scope

- `StoreBackend::Git`, `remote` and `branch` on `[[types]]`.
- Clone-on-first-read, `fetch` coverage, the gitignore guard.
- Explicit write refusal.
- TUI store parser and background-refresh parity.

### Out of Scope

- Writes -- STORY-282.
- `extends` -- STORY-284.
- Credentials. Whatever `git` already does for that URL is what lazyspec does: no token storage, no auth prompts.
- Documenting the `git` / `git-ref` split in `--help` and the README. That lands once, in STORY-284, rather than being rewritten by each slice.

## Notes

The store parser exists at two independent sites -- `src/cli/config.rs:1185` and `src/tui/state/app.rs:162`. Both need the new variant; only the first is obvious.

Hidden cost worth knowing before starting: `GitRefOps` (`src/engine/git_ref.rs:13-45`) has no clone or worktree operation, so a new op threads through `Store::load_with_fs` (`store.rs:115`), whose signature is touched in eleven places across `src/engine/store.rs`, `src/tui/state/settings_guard.rs` and `tests/integration/cli_git_ref_show_test.rs`. Still a few days.

The web view is free -- it reads through `Store::load` (`src/web/render.rs:562`).

A managed clone can be stale, so a document can look current and not be. Accepted: the same is true of every cached store here, and `fetch` is the remedy.
