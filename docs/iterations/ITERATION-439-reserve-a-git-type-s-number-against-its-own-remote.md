---
title: Reserve a git type's number against its own remote
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-09
tags: []
related:
- implements: STORY-282
- blocks: ITERATION-440
reviewed: 10607d51537a582ab71e87313ed2fe9d426e9b15
---
## Objective

A `git` type with `numbering = "reserved"` claims its number as a `refs/reservations/*` ref on the type's own remote, from the clone, so two repos writing concurrently cannot allocate the same ID; and a rejected `create`, once `fetch`ed, re-runs onto a fresh ID.

## Satisfies

STORY-282 AC7, AC8. AC9, AC10 deferred -- see Out of scope.

## Context

- Story + ACs: STORY-282. Contract: RFC-072 Design "The git store", second bullet ("IDs are reserved against the type's remote").
- **The one wrong pair.** `fs_ops::create_document` (`src/engine/fs_ops.rs:97-201`), `Reserved` arm at `:122-153`, calls `reservation::reserve_next(root, &reserved_cfg.remote, ..)` at `:129-136`: repo is the project root, remote is `[numbering.reserved].remote` (`ReservedConfig` at `src/engine/config.rs:527-533`, default `origin`). For a `git` type the repo must be the clone and the remote the clone's `origin`, which `clone_repo` (`src/engine/git_ref.rs:264-278`) set to `type_def.remote`. `docs_dir` is already right: ITERATION-437 passes the clone-relative `dir`, so `target_dir` (`:109`) is `doc_root`.
- **One parameter, not a second function:** `reservation_repo: Option<&Path>` on `create_document`. `None` -> today's `(root, &reserved_cfg.remote)`; `Some(clone)` -> `(clone, "origin")`. `format` and `max_retries` still come from `reserved_cfg` (`:123-128`), which stays required. Callers: `FilesystemStore::create` (`src/engine/store_dispatch.rs:489-500`) and the filesystem branch of `ops::create` (`src/engine/ops/create.rs:205-216`) pass `None`; `GitStore::create` passes `Some(&clone_root)`.
- **`reserve_next` needs nothing** (`src/engine/reservation.rs:226-278`): `ls_remote` (`:116`) and `push_ref` (`:188`) take `repo_root` and `remote` by name; `refs/reservations/<PREFIX>/<n>` on a non-bare remote is not a checked-out ref, so `updateInstead` is irrelevant there. `cleanup_local_ref` (`:211`) removes the local ref from the clone on rejection as it does from the project root today.
- **AC7 is ITERATION-437's rollback plus `fetch`.** `GitSync::sync` (`src/engine/sync.rs:221-260`) runs `update_clone` -> `reset --hard FETCH_HEAD`, so the remote's winner arrives in the clone; a re-run of `create` then scans it -- `next_number` (`src/engine/template.rs:28-46`) for incremental, `remote_max.max(local_max) + 1` (`reservation.rs:235-239`) for reserved -- and lands on n+1. A reservation claimed by a run whose doc push was then rejected stays on the remote; the retry claims the next one. That is `reserve_next`'s contract already, not a gap.
- Progress: `GitStore::create` passes `|_| {}` as `FilesystemStore::create` does (`:499`); the reservation spinner (`src/main.rs:195-199`) shows nothing for a `git` type. Same as every registry-routed backend today.
- Fixtures: `shared_repo()` and the clone `user.*` setup (ITERATION-437); `reservation_test.rs:7-24` `seed_ref_on_bare` seeds a reservation ref; a second project is a second `TempDir` with `write_project_config` (`tests/integration/git_store_test.rs:222`) pointing at the same remote.

## Tasks

1. Test-first, `fs_ops.rs` tests: `create_document` with `Reserved` and `reservation_repo: Some(dir)` reaches `reserve_next` with `dir` and `origin` -- assert through the error text when `dir` is not a repository (`git ls-remote` fails naming `origin`), no network. Add the parameter; thread `None` through the two callers.
2. Test-first, `git_store_test.rs`: (a) a `git` type with `numbering = "reserved"`, `[numbering.reserved] remote = "no-such-remote"`: `create` succeeds -- proof the type's remote won -- with `RFC-002-*.md` in the clone, `git -C <remote> for-each-ref refs/reservations/RFC/` listing `2`, and one new commit on `next`; (b) seed `refs/reservations/RFC/7` on the remote, create -> `RFC-008`; (c) two projects on one remote, reserved: A creates `RFC-002`; B (stale) creates -> `Err` naming `lazyspec fetch`, B's clone bytes unchanged; `lazyspec fetch` in B, create again -> the id is neither `RFC-002` nor one the remote's refs already hold, and it lands on the remote.
3. `GitStore::create` passes the clone root. Green.
4. Test-first, `git_store_test.rs` (incremental, AC7 through the binary): project A `create rfc "A2" --json` -> `RFC-002`; project B `create rfc "B2" --json` -> exit non-zero, stdout empty; `lazyspec fetch --json` in B; `create rfc "B2" --json` -> `RFC-003`, `synced: true`; `git -C <remote> ls-tree --name-only next docs/rfcs` lists 001, 002, 003.

## Out of scope

- `reservations list`/`prune` (`src/cli/reservations.rs:33`, `:126`) keep reading `[numbering.reserved].remote` from the project root; a `git` type's reservations live on its own remote and are not listed. Story Notes: `reservations` is ref-only.
- A spinner for the git-type reservation round trip.
- Sqids `reserved` format on a git type -- same code path (`fs_ops.rs:139-150`), not separately tested.
- AC9, AC10 -> ITERATION-440.

## Principles/conventions

`cargo run -q -- convention`. Principle 6: one `Option<&Path>`, no `ReservationTarget` struct for two callers of one function. DICTUM-004: every remote is a TempDir; the "wrong remote" is a name that resolves nowhere. Principle 3: `create_document` stays the one place numbering happens.

## Verification

ITERATION-437's scratch project with `numbering = "reserved"` on the `git` type and `[numbering.reserved] remote = "bogus"`: `cargo run -q -- create rfc "r" --json | jq .id` prints a fresh id; `git -C <remote> for-each-ref refs/reservations/` shows it; `git ls-remote bogus` in the project root would fail, proving the project's remote was never asked. Move the remote ahead, `create` fails, `cargo run -q -- fetch --json`, `create` again succeeds with a higher id.
