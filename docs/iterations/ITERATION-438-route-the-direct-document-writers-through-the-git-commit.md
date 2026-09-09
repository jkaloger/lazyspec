---
title: Route the direct document writers through the git commit
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-09
tags: []
related:
- implements: STORY-282
- blocks: ITERATION-439
reviewed: 10607d51537a582ab71e87313ed2fe9d426e9b15
---
## Objective

Every writer that rewrites a document file without going through `DocumentStore` -- `link`/`unlink`, `ignore`/`unignore`, `pin`, the `fix` modes, the TUI's tag write and external-edit save -- commits and pushes when the file sits in a `git` type's clone, through the ITERATION-437 op.

## Satisfies

STORY-282 AC2 (the remainder: `link`, `unlink`, `ignore`, `unignore`, `pin`, `fix`), AC3. AC7-AC10 deferred -- see Out of scope.

## Context

- Story + ACs: STORY-282 (Notes paragraph one: the write-command list). Contract: RFC-072 Design "The git store" paragraph three.
- **One free function, one place.** In `src/engine/git_store.rs` (ITERATION-437): `pub fn commit_if_git_backed(root: &Path, config: &Config, doc_path: &Path, ops: &dyn GitRefOps, message: &str) -> Result<()>`. `doc_path` is root-relative; the type is the third component of `.lazyspec/cache/<type>/...`, read exactly as `push_if_git_ref_backed` reads it (`src/engine/ops/link.rs:992-1009`); any other path, or a type whose `store != Git`, is `Ok(())`. Otherwise `commit_and_push` on the clone root with the ITERATION-437 context wrapper -- share that wrapper with `GitStore`, do not spell the message twice. A second call for a clone already committed is a no-op (`diff --cached --quiet`), so callers that write several files call it per file and let the op dedupe.
- **`link`/`unlink`** (`link.rs:38-190`, `:656-791`): the cache rewrite at `:142-167` / `:757-770`, then `push_if_git_ref_backed` at `:169` / `:772`. Add `commit_if_git_backed(root, config, &resolved_from, &GitCli, ..)` beside each, hardcoding `GitCli` as `:1020` does. Delete `refuse_git_source` (`:595-605`, calls at `:95`, `:713`) and `git_write_refusal` (`src/engine/store_dispatch.rs:3110-3118`) -- the last two readers.
- **`ignore`/`unignore`** (`src/cli/ignore.rs:8-15`, `:17-26`): `rewrite_frontmatter` on `root.join(resolved)`. Gain `config: &Config` and `git: &dyn GitRefOps`; call sites `src/main.rs:487`, `:504` pass `&config`, `&GitCli`; tests `tests/integration/cli_ignore_test.rs:11`, `:32` pass `&fixture.config()`, `&MockGitRefClient::new()` (`test-support` feature, as `cli_fix_governs_test.rs` does).
- **`pin`** (`src/cli/pin.rs:124-222`) already holds `config` and `git: &dyn GitRefOps` (`:126-127`); body write at `:192-193`, `stamp_reviewed` at `:196`. One call after `:196` with `&doc.path`. `run` returns before writing when `HEAD` cannot be read (`:168-170`); keep that order.
- **`fix`.** Every mode writes through `fs` inside `record_write` (`src/engine/ops/fix.rs:32-43`): `fields.rs:79-82`, `relations.rs:69-70`, `governs.rs:42-44`, `status.rs:54-56` (skips non-filesystem at `:41`; leave it), `conflicts.rs:125-129`/`:150-154` (renames), `cascade.rs:99`, `src/cli/fix/renumber.rs:195`, `:357`, `:382`, `:416`. Commit after the plan, before output, in `cli/fix.rs::run` (`:51`), `run_governs` (`:135`, already takes `git: Box<dyn GitRefOps>`), `run_renumber` (`:192`): for each result with `written: true`, `commit_if_git_backed` on its `path` (`new_path` for renames; `references_updated[].file` for cascade). `run` and `run_renumber` gain a `git: &dyn GitRefOps` param; `main.rs:644`, `:629` pass `&GitCli`. A commit `Err` supersedes the output: print the error, return 1 -- the rollback made every `written: true` false, so printing them would be the DICTUM-006 lie. The TUI fix runner (`src/tui/infra/event_loop.rs:1148`) calls the engine planner directly; give it the same post-plan commit.
- **TUI (AC3).** `update_tags` (`src/tui/state/app.rs:381-391`) is a direct `rewrite_frontmatter`; callers `:2951` (thread) and `:3001` -- one `commit_if_git_backed` after each with `&GitCli`. External-edit save: `event_loop.rs:1031-1058` spawns `try_push_gh_edit`, `try_push_git_ref_edit` (`:144-172`), `try_push_clickup_edit`; add `try_push_git_edit` in the `git_ref` arm's shape, with an `_with(ops)` split like `try_push_clickup_edit_with` (`:191-236`) so a `MockGitRefClient` drives it. The `set_provenance` call at `app.rs:3564-3566` (the story's `:3562`) reaches `GitStore::set_provenance` through the registry since ITERATION-437; `ops::create`/`update`/`delete`/`link` at `:2904`, `:2983`, `:3389`, `:3067`, `:2958`, `:3009`, `:3667` likewise. Nothing else in `src/tui` writes a document (`rewrite_frontmatter` has one TUI reader, `:19`/`:383`).
- Fixtures: `shared_repo()` with `updateInstead` and the clone `user.*` setup from ITERATION-437; `lazyspec()` (`tests/integration/git_store_test.rs:247`).

## Tasks

1. Test-first, `git_store.rs` `#[cfg(test)]`: (a) `commit_if_git_backed` on `.lazyspec/cache/rfc/docs/rfcs/RFC-001-a.md` with a `git` type -> one `commit_and_push:<root>/.lazyspec/cache/rfc:...` call; (b) `docs/rfcs/RFC-001-a.md` (filesystem) -> zero calls; (c) `.lazyspec/cache/story/STORY-1.md` with `store = "github-issues"` -> zero calls; (d) queued `Err` -> `Err` naming remote, branch, `lazyspec fetch`.
2. Add the function; `link.rs` calls and the two deletions; `ignore.rs`, `pin.rs` calls; `fix.rs` and `renumber` post-plan commits; TUI `update_tags` callers and `try_push_git_edit`. Green on the existing suites after the signature changes.
3. Test-first, `git_store_test.rs`: with the clone user-configured, each of `link_with_config`, `unlink_with_config`, `ignore`, `unignore`, `pin::run`, `fix::run` (a doc missing `author`), `fix::run_renumber` adds exactly one commit to `git -C <remote> log next` and the file content at `next:` carries the change; after committing straight into the remote, `link_with_config` returns `Err` naming `lazyspec fetch` and the clone's `RFC-001-a.md` bytes are unchanged.
4. Test-first, `event_loop.rs` beside the `try_push_clickup_edit_with` tests: `try_push_git_edit_with` on a `git` doc -> one `commit_and_push` call; on a filesystem doc -> none.
5. `main.rs` call sites; `clippy -D warnings`.

## Out of scope

- `fix --status` on a `git` doc stays skipped (`status.rs:41`); the story asks that rewrites commit, not that more rewrites happen.
- `fix --config` (`config.rs:526`) writes `.lazyspec.toml`, not a document.
- The `git-ref` external-edit arm's fire-and-forget (`event_loop.rs:1043-1050`) -- `try_push_git_edit` reports its `Err` through the same `GhPushResult` channel; no new event.
- A per-run commit message naming every file -- `fix <mode>` is enough.
- AC7, AC8 -> ITERATION-439. AC9, AC10 -> ITERATION-440.

## Principles/conventions

`cargo run -q -- convention`. Principle 3: the commit is engine code called from CLI and TUI; neither frontend spawns git itself. Principle 6: one free function, no trait for the direct writers. DICTUM-006: a rolled-back write is not reported as written. DICTUM-004: unit tests on the mock, real git only in `git_store_test.rs`.

## Verification

ITERATION-437's scratch project: `cargo run -q -- ignore RFC-0NN && git -C <remote> log -1 --format=%s` reads `ignore RFC-0NN`; `cargo run -q -- link RFC-0NN related-to RFC-001 --json | jq .synced` is `true`; `cargo run -q -- fix --json` on a doc missing `author` reports `written: true` and the remote gained a commit; `git -C .lazyspec/cache/rfc status --porcelain` is empty throughout.
