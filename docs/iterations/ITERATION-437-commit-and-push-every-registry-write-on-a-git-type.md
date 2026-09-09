---
title: Commit and push every registry write on a git type
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-09
tags: []
related:
- implements: STORY-282
- blocks: ITERATION-438
reviewed: 10607d51537a582ab71e87313ed2fe9d426e9b15
---
## Objective

A `GitStore` registered under `StoreBackend::Git` writes into the managed clone, commits, pushes to the declared branch; a rejected or failed push is an error naming remote, branch and `lazyspec fetch`, with the clone reset to its pre-command bytes.

## Satisfies

STORY-282 AC1, AC4, AC5, AC6; AC2 for the registry-routed commands (`update`, `tag`, `delete`, `provenance add`/`remove`). AC2 for `link`/`unlink`/`ignore`/`unignore`/`pin`/`fix`, AC3, AC7-AC10 deferred -- see Out of scope.

## Context

- Story + ACs: STORY-282. Contract: RFC-072 Design "The git store" (paragraph three and the two bullets), Decision 5.
- **One new op on `GitRefOps`** (`src/engine/git_ref.rs:13-73`): `fn commit_and_push(&self, clone: &Path, branch: Option<&str>, message: &str) -> Result<()>`. `GitCli` (`:113`): `rev-parse HEAD` first; `add -A`; `diff --cached --quiet` exit 0 means nothing changed, return `Ok` without a commit; `commit -q -m <message>`; `push origin HEAD` or `HEAD:<branch>`, built like `update_clone` at `:280-298` (`GIT_TERMINAL_PROMPT=0`, `output_with_timeout`); any push failure -> `reset --hard <saved head>` then `bail!` with git's trimmed stderr. Rollback lives inside the op so the clone is byte-identical whatever the failure (AC6); `reserve_next`'s `cleanup_local_ref` (`src/engine/reservation.rs:211-224`, called at `:256` and `:260`) is the precedent. Three impls: `GitCli`, `MockGitRefClient` (`:503`; a `commit_and_push_results` queue, `pop_or_default` at `:685`, a `commit_and_push:<clone>:<branch|default>:<message>` entry in `calls`), `RenamingGit` (`tests/integration/cli_fix_governs_test.rs:35`, `unreachable!`).
- **No `LocalOnly` (AC5).** `GitRefStore::handle_push_result` (`src/engine/git_ref_store.rs:109-124`, the story's `:138`) turns an unreachable remote into `PushOutcome::LocalOnly`; the `git` store never does. Every op error is `Err`, wrapped once in the engine: `.with_context(|| format!("pushing to {remote} ({branch}): the remote may have moved; run `lazyspec fetch` and retry"))`, `branch.unwrap_or("default branch")` as `clone_git_store` spells it (`src/engine/store.rs:164-170`). Success is `PushOutcome::Synced`. Amend the `PushOutcome` doc comment (`src/engine/store_dispatch.rs:51-55`) with the divergence.
- **`GitStore` is `FilesystemStore` plus a commit.** New `src/engine/git_store.rs` (file-per-concern; `store_dispatch.rs` is 7k lines): `pub struct GitStore { pub root: PathBuf, pub config: Config, pub ops: Box<dyn GitRefClient> }`. STORY-283 made every git doc's `meta.path` root-relative under `.lazyspec/cache/<type>/<dir>/` (`tests/integration/git_store_test.rs:95-100`), so `fs_ops::update_document_with_type` (`src/engine/fs_ops.rs:307`), `delete_document` (`:268`) and `FilesystemStore::set_provenance` (`store_dispatch.rs:537-570`) already write the right file: delegate `update`/`delete`/`set_provenance` to a `FilesystemStore { root, config }` (`:126-129`), then commit. `sync_tags` is commit only -- `cli/tag.rs:22-34` has already rewritten the cache file. `create`: `fs_ops::create_document` (`:97-201`) joins `dir` onto `root` at `:109`, so pass `doc_root(root, type_def)` (`store.rs:118-129`) stripped of `root` as `dir`; `FilesystemStore::create` drops `_body` (`:487`) because `ops::create` applies it only on the filesystem branch (`src/engine/ops/create.rs:218-220`) -- `GitStore::create` must call `fs_ops::replace_body` (`:87`) itself before committing. Commit message: `<verb> <id>`.
- **Clone root and branch** are `root.join(".lazyspec/cache").join(&type_def.name)` and `type_def.branch.as_deref()`, as `Store::load_with_fs` computes them (`store.rs:195`, `:162`). `remote` is `Some` by the parse check at `src/engine/config.rs:2019-2025`; `expect` with that reason.
- **Registration:** `build_registry` (`store_dispatch.rs:2972`) replaces the `UnavailableStore` block at `:3090-3105` with `GitStore { ops: Box::new(GitCli), .. }`. `ops::create` already routes `Git` to the registry (`ops/create.rs:85-94`); `ops::update` (`ops/update.rs:130-150`), `ops::delete` (`ops/delete.rs:27-46`), `engine::provenance::set_provenance` (`src/engine/provenance.rs:44-49`) and `propagate_tags` (`cli/tag.rs:80-98`) follow for free. Drop `refuse_git` from `cli/tag.rs:21`, `:49`, `:62-68`. Keep `git_write_refusal` (`:3110-3118`) and `refuse_git_source` (`src/engine/ops/link.rs:595-605`) for `link` until ITERATION-438.
- **`--json` on rejection (AC4):** `run_json_with_body` (`src/cli/create.rs:35-68`) reads the file only after `run_with_body` returns `Ok`; `main` is `-> anyhow::Result<()>` (`src/main.rs:19`), so `Err` is `Error: ...` on stderr, exit 1, empty stdout. Nothing to add; assert it.
- **Fixtures.** `shared_repo()` (`git_store_test.rs:40-55`) is a non-bare repo with `main` checked out: a push to it fails on `receive.denyCurrentBranch` until the fixture sets `receive.denyCurrentBranch updateInstead`, which also keeps its "commit straight into the remote" idiom (`:284-286`) as the way to move the remote ahead. The clone has no `user.*`: after the first `Store::load` (or a first `lazyspec list`), run `git config user.email/user.name` in `<root>/.lazyspec/cache/rfc` before writing (DICTUM-004: no reliance on the host's global config). `lazyspec()` (`:247`) drives the binary.

## Tasks

1. Add `commit_and_push` to the trait and three impls.
2. Test-first, `git_store.rs` `#[cfg(test)]` with `MockGitRefClient` and a TempDir holding `.lazyspec/cache/rfc/docs/rfcs/RFC-001-a.md` (no real git): (a) `update` -> file carries the new title, `calls` holds one `commit_and_push:<root>/.lazyspec/cache/rfc:next:...`; (b) `create` -> `RFC-002-<slug>.md` under the clone's `docs/rfcs`, body applied, returned `path` relative to `root`, `id == "RFC-002"`, `push_outcome == Synced`; (c) `create` with `branch: None` logs `default`; (d) queued `Err("! [rejected]")` on `update` -> `Err` whose `{:#}` names the remote, `next`, and `lazyspec fetch`; (e) `delete` -> file gone, one commit call; (f) `sync_tags` -> one commit call, file untouched.
3. `GitStore`, registration, `tag.rs` cleanup. Green. Replace `build_registry_refuses_writes_to_git_types` (`store_dispatch.rs:5715`) with "registers a `GitStore`".
4. Test-first, `git_store_test.rs`, replacing `every_write_to_a_git_type_is_refused_before_the_clone_is_touched` (`:160-216`): (a) `create::run` -> file exists under `doc_root`, `git -C <remote> log --oneline next` gained one commit and `git -C <remote> show next:docs/rfcs/<file>` returns it; (b) `ops::update`, `cli::tag::tag_add_with_config`, `ops::delete`, `provenance::set_provenance` each add one remote commit; (c) commit `RFC-002-b.md` straight into the remote after the clone, then `create::run` -> `Err` naming the remote path, `next`, `lazyspec fetch`; `git -C <clone> rev-parse HEAD` unchanged, `status --porcelain` empty, no new file under `docs/rfcs`, `template::next_number` on that dir still returns 2; (d) same setup through the binary: `lazyspec create rfc "C" --json` exits non-zero, stdout is empty, stderr names `lazyspec fetch`.
5. `clippy -D warnings`; the `Git` arms from ITERATION-433 stay exhaustive.

## Out of scope

- `link`/`unlink` (`link.rs:142`, `:757`), `ignore`/`unignore`, `pin`, `fix` modes, TUI `update_tags` and external-edit push -> ITERATION-438. `link` keeps refusing until then.
- AC7 (retry after `fetch`) and AC8 (reservation) -> ITERATION-439. `NumberingStrategy::Reserved` on a `git` type still reserves against `[numbering.reserved].remote` from the project root.
- AC9, AC10 -> ITERATION-440. `--parent` on a `git` child is dropped by the early return at `ops/create.rs:87-94`; ITERATION-440 owns that.
- `review_stamp` (`ops/update.rs:74-79`) excludes `Git`; a git doc can hold `reviewed`, but the story does not ask for it.
- Distinguishing rejected from unreachable in the message. Decision 5: both exit non-zero; one message.

## Principles/conventions

`cargo run -q -- convention`. Principle 4 / DICTUM-002: one `GitRefOps` method, fake in `test_support`. Principle 6: `GitStore` wraps `FilesystemStore`, no second write path. DICTUM-004: real git only against the TempDir remote; the unit tests never spawn git. DICTUM-006: the error is the `anyhow` chain a human reads; exit code from `main`.

## Verification

Scratch project with `store = "git"`, `remote = <path to a TempDir clone of this repo with receive.denyCurrentBranch=updateInstead>`, `dir = "docs/rfcs"`: `cargo run -q -- create rfc "x" --json | jq .synced` is `true` and `git -C <remote> log -1 --format=%s` reads `create RFC-0NN`. Commit in the remote, then `cargo run -q -- update RFC-0NN --title y --json; echo $?` prints nothing on stdout, `1`, and stderr names `lazyspec fetch`; `git -C .lazyspec/cache/rfc status --porcelain` is empty.
