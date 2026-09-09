---
title: Compare resolved repos in the create --parent guard
type: iteration
status: accepted
author: Jack Kaloger
date: 2026-09-09
tags: []
related:
- implements: STORY-282
reviewed: 10607d51537a582ab71e87313ed2fe9d426e9b15
---
## Objective

`create --parent` on a `git` child reaches the same-store guard instead of being dropped, the guard refuses two `git` types whose `remote`/`branch` differ, and a permitted subdir child is written into the parent's clone and pushed.

## Satisfies

STORY-282 AC9, AC10.

## Context

- Story + ACs: STORY-282 (Notes paragraph two). Contract: RFC-072 Design "The git store", last paragraph.
- **`--parent` is silently dropped today.** `ops::create::run_with_body` returns for `StoreBackend::Git` at `src/engine/ops/create.rs:85-94`, ahead of the `parent` branch at `:96-101`; a `git` child asked for a parent lands flat. ITERATION-435 put the return there so a refused type could not fall through. Move the `parent` branch ahead of it: `create_with_parent` (`:238-345`) then runs for git children, and its guard is what AC9 wants applied unchanged.
- **The guard** (`:262-271`) compares `store` discriminants. Add `fn same_repo(a: &TypeDef, b: &TypeDef) -> bool` = equal `store`, `remote`, `branch`. `remote`/`branch` are `None` on every non-git type by the parse check (`src/engine/config.rs:2026-2032`), so the comparison degrades to today's for them; two `git` types differ on the resolved repo when either field differs. Extend the message to name both remotes when both are `Git`.
- **The subdir path is already repo-correct.** `parent_path = root.join(&parent_meta.path)` at `:304` (the story's `:293`): since STORY-283 a git doc's `meta.path` is root-relative under `.lazyspec/cache/<type>/<dir>/` (`tests/integration/git_store_test.rs:95-100`), so the join lands in the parent's clone, the promotion `fs::rename` (`:329-333`) and `fs_ops::create_child_in_dir` (`:336-344`) write there. Not a bug; pin it with a test.
- **The commit is the parent's.** After `create_child_in_dir`, `commit_if_git_backed(root, config, &parent_meta.path, &GitCli, ..)` (ITERATION-438) -- keyed on the parent's path so the clone that was written is the clone committed, even when child and parent are two `git` types sharing a remote. Rename and new file land in one commit (`add -A`). The GitHub branch at `:273-302` returns before this and is untouched.
- `create_with_parent` returns `PathBuf` and the caller maps `PushOutcome::Synced` (`:100`); a rejected push is `Err`, so nothing changes there.
- Fixtures: `cli_child_test.rs:157` `create_with_parent_cross_store_rejected_before_mutation` is the guard's existing test shape; `store_from_with_config` (`src/engine/store.rs:1065`) for a unit test; `shared_repo()` + clone `user.*` (ITERATION-437), `git_config` (`git_store_test.rs:57-68`) extended to take a type list.

## Tasks

1. Test-first, `create.rs` `#[cfg(test)]` with `store_from_with_config`: (a) `same_repo` true for two filesystem types, true for two `git` types with equal `remote` and `branch`, false when `remote` differs, false when only `branch` differs; (b) `create_with_parent` with a filesystem parent and a `git` child -> `Err` containing `different stores` (AC9, unchanged text); (c) two `git` types, remotes `/a.git` and `/b.git`, a parent doc seeded under `.lazyspec/cache/a/docs/a/` -> `Err` naming both remotes, no file written under either cache dir.
2. `same_repo`, the message, the branch reorder, the commit call. Green.
3. Test-first, `git_store_test.rs`: (a) one `git` type, `create::run_with_body(.., parent: Some("RFC-001"), ..)` -> `RFC-001-a/index.md` and `RFC-001-a/RFC-002-*.md` under `doc_root`, `git -C <remote> ls-tree -r --name-only next docs/rfcs` shows both, one new commit; `Store::load` lists RFC-002 with `parent_of` RFC-001; (b) two `git` types on the same remote and branch (`spec` with `dir = "docs/specs"`, `rfc` with `dir = "docs/rfcs"`): a `spec` child of RFC-001 lands in the `rfc` clone's `docs/rfcs/RFC-001-a/` and on the remote; (c) two `git` types on two remotes -> `Err` naming both, neither remote gains a commit, both clones `status --porcelain` empty.
4. Through the binary: `create rfc "child" --parent RFC-001 --json` -> `.path` under `.lazyspec/cache/rfc/docs/rfcs/RFC-001-a/`, `synced: true`.

## Out of scope

- `parent_of` a child in a sibling clone of the same remote appearing under the child type's own clone -- it appears where the parent does, as filesystem children do today.
- `link`'s cross-repo relations; a relation is a frontmatter line, not a file placement.
- Reserved numbering for subdir children (`fs_ops.rs:222-233` `_ => None`) -- pre-existing.
- The `sub-issue link rejected` wording at `:264` -- keep it; add the remotes, do not rewrite it.

## Principles/conventions

`cargo run -q -- convention`. Principle 6: three field comparisons in one private fn; no `ResolvedRepo` type. DICTUM-001: `same_repo` reads the fields, no `match` on `StoreBackend` to keep exhaustive. DICTUM-004: two TempDir remotes for the refusal, one for the permitted case.

## Verification

ITERATION-437's scratch project: `cargo run -q -- create rfc "child" --parent RFC-0NN --json | jq .path` is under `.lazyspec/cache/rfc/docs/rfcs/RFC-0NN-*/`; `git -C <remote> log -1 --stat` shows the rename to `index.md` and the child in one commit. Add a second `git` type with a different `remote`: `cargo run -q -- create <other> "x" --parent RFC-0NN; echo $?` prints an error naming both remotes and `1`.
