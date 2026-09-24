---
title: 'git store: per-type clones and push-on-write reject a second create'
type: bug
status: in-progress
author: Jack Kaloger
date: 2026-09-24
tags: []
related:
- related-to: STORY-282
- related-to: RFC-072
reviewed: 2c56ed0b69d66e46b6caa0309f9434b4294d8ad0
---

## Summary

Two `git`-store types on the same remote + branch. Create on one, then create on the other -> second push rejected "fetch first". Each type owns its own clone, every write pushes, nothing refreshes a clone before pushing. Any teammate push hits the same wall, one type or many.

## Reproduction

Observed in a client repo (`risk`, `assumption`, `issue`, `dependency` all `store = "git"`, one shared remote, default branch). Reflog timeline:

| time | clone | event |
|---|---|---|
| 10:41:18 | risk | reset to FETCH_HEAD `564c018` |
| 10:41:45 | assumption | commit `create ASSUMPTION-003` -> pushed `94214f3` |
| 10:42:19 | risk | commit `create RISK-008` on stale `564c018` -> push rejected |
| 10:42:22 | risk | rollback to `564c018` |

Minimal: two git types, same remote. `create a "x"`, then `create b "y"` -> second fails.

## Expected

Writes to any git type on a shared remote land regardless of what sibling types or other writers pushed meanwhile. Real content conflicts surface to the human with enough info to resolve them.

## Actual

`Error: pushing to <remote> (default branch): the remote may have moved; run lazyspec fetch and retry`. Commit rolled back.

## Root cause

- Clone path keyed by type name, not repo: `.lazyspec/cache/<type>` built at `src/engine/store.rs:125`, `src/engine/git_store.rs:23`, `src/engine/git_store.rs:94`, `src/engine/sync.rs:250`. N types on one remote = N clones of one branch.
- Every write pushes immediately (`GitRefOps::commit_and_push`, `src/engine/git_ref.rs:306`). Clone only refreshed by `fetch`/poll (`update_clone`, `src/engine/git_ref.rs:286`). Push from clone A leaves clone B behind; B's next push is non-fast-forward.
- Push-on-write was an accepted RFC-072 trade-off (Decision 5, "push on every write is slow" risk). This bug reverses it.

## Fix

Change the write model: one clone per repo, writes commit locally, explicit `push` rebases then pushes.

### Acceptance criteria

1. **Shared clone.** Given two git types with the same `remote` + `branch`, when either is read or written, then both resolve into one clone at `.lazyspec/git/<slug>/` (readable remote + branch slug plus a stable hash of the raw pair) and each type's `dir` resolves inside it. `commit_if_git_backed` (`src/engine/git_store.rs:39`) maps a doc path to its clone by prefix, not by `components().nth(2)`.
2. **Local commit, no push.** Given a git type, when any mutating command runs (CLI or TUI, every writer listed in STORY-282), then the change is committed in the shared clone and not pushed. Output reports `synced: false` (`PushOutcome::LocalOnly`) at exit 0. Rejection rollback in `commit_and_push` is removed.
3. **`lazyspec push`.** Given shared clones with unpushed commits, when I run `lazyspec push [--json]`, then each is fetched, rebased onto `origin/<branch>`, and pushed. Output lists per clone: path, remote, branch, commits pushed. Nothing to push -> exit 0, reports zero.
4. **Conflicts left to the human.** Given a rebase conflict during `push`, then the rebase is aborted, local commits are kept, the command exits non-zero, and the error (and `--json` error) names the clone path, the conflicted files, and the commands to resolve there (`git -C <path> pull --rebase`, then `lazyspec push`). Other clones in the same run still push.
5. **Rebase-safe fetch.** Given unpushed local commits, when `fetch` (or the TUI poll) refreshes a git clone, then it rebases onto the fetched head instead of `reset --hard FETCH_HEAD`; no local commit is lost. Conflict handling as AC4.
6. **Duplicate IDs block push.** Given local creates on an `incremental` git type, when `push` has rebased onto the remote and a local create's ID now collides with another doc of that type, then nothing is pushed, the command exits non-zero, and the error (and `--json` error) names the clone path and the colliding docs; the human renumbers and re-runs `push`. `reserved` numbering avoids this by claiming IDs at create time.
7. **Unpushed work visible.** `status --json` gains `git_stores: [{path, remote, branch, unpushed}]`. TUI header shows the unpushed count for git clones and a keybind runs push, surfacing AC4 errors with the clone path.
8. **Migration.** Given old per-type clones under `.lazyspec/cache/<type>/`, when the shared clone is first needed it is cloned fresh; `fetch` deletes old per-type git clones that have no unpushed commits and warns (keeping them) otherwise.
9. **Docs.** README documents `push` and the local-commit write model. RFC-072 gets a note that this bug supersedes Decision 5 and the push-on-every-write risk.
10. **Extends clone writes.** Given a URL `extends` (clone at `.lazyspec/cache/config`), when a mutating command writes a doc into it (`Config::docs_root`, `src/engine/config.rs:2298`), then the write is committed locally in that clone like a git-store write, `lazyspec push` pushes it, and `fetch` rebases rather than discarding it. Today those writes are never committed and the next `fetch` hard-resets them away.

### Tasks

1. Clone-path helper keyed by remote + branch; route `doc_root`, `Store::load` first-read clone, `commit_clone`, `GitStore::create` (reservation repo), `sync_git_clone` through it. Rework `commit_if_git_backed` path mapping. (AC1)
2. Split `GitRefOps::commit_and_push` into `commit` (local) and a `push` path (fetch + rebase + push, abort on conflict, report conflicted files). Extend `MockGitRefClient` queues for both. (AC2, AC3, AC4)
3. `update_clone` -> fetch + rebase; shared conflict error type with task 2. (AC5)
4. Duplicate-ID check between rebase and push, scoped to docs added in `origin/<branch>..HEAD`; error names clone path + colliding docs. Integration test: two clones create the same incremental ID, second `push` refuses. (AC6)
5. `lazyspec push` CLI command with `--json`; `status --json` `git_stores`. (AC3, AC7)
6. TUI: unpushed count in header, push keybind, error surfacing. (AC7)
7. Old per-type clone cleanup in `fetch`. (AC8)
8. Integration test on real repos: two git types, one remote; create on each; `push` lands both. Second test: two clones of the remote edit one doc -> `push` exits non-zero naming the clone path, local commit kept. (AC1-5)
9. README + RFC-072 note. (AC9)
10. Treat the URL-`extends` clone as a push target: `commit_if_git_backed` commits writes under `.lazyspec/cache/config/`; `push` includes it. Integration test: extends write survives `fetch` and lands on `push`. (AC10)

### Out of scope

- TUI never polls a project whose only remote type is `git` (`has_pollable_types`, `src/tui.rs:15`). Separate bug.
- Automatic conflict resolution or renumbering. ID collisions and content conflicts are the human's.
