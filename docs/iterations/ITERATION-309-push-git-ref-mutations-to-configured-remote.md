---
title: Push git-ref mutations to configured remote
type: iteration
status: accepted
author: agent
date: 2026-07-17
tags: []
related:
- implements: STORY-218
- blocks: ITERATION-310
---

## Objective

Every git-ref mutation pushes to configured remote; TUI edit paths included; offline behaviour defined; README updated.

## Satisfies

STORY-218 AC2, AC4, AC5. (AUDIT-019 F1, F4, F5)

## Context

- Depends: remote-config iteration (store carries `remote`)
- Mutations local-only: `src/engine/git_ref_store.rs:100-355` (create/update/set_provenance/delete/sync_tags)
- Client ops exist unused: `push_ref`/`push_ref_with_lease`/`delete_remote_ref` (`src/engine/git_ref.rs:216-278`)
- Bypass paths: `try_push_git_ref_edit` (`event_loop.rs:119-127`), `sync_git_ref_link_edit` (`ops/link.rs:~710-737`, inline `GitCli`)
- Unreachable-remote precedent: `reservation.rs:57-67`
- README contradiction: `README.md:863-865` ("no automatic remote push")

## Tasks

1. `create` → `push_ref` after local commit. `update`/`set_provenance`/`sync_tags` → `push_ref_with_lease(remote, refname, new_sha, Some(old_sha))` after CAS; lease rejection surfaces as existing conflict error. `delete` → `delete_remote_ref`.
2. Route TUI body-edit + link-edit through push-enabled store methods (kill inline `GitCli` path in `ops/link.rs`).
3. Offline: mutation succeeds locally, push failure → warning w/ retry hint (matches reservation fallback shape); consistent across all mutations. Fake-client test.
4. Fake-client tests: push per mutation, lease args, delete. `cargo test`.
5. README `:863-865` rewrite: live by default, offline semantics.

## Out of scope

Number allocation (next iteration). Live on/off toggle (AUDIT-019 F6 — not until requested).

## Verification

`cargo test`. Manual: git-ref doc update → `git ls-remote origin 'refs/lazyspec/*'` shows new SHA.

