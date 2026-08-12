---
title: re-check remote body before relation and native-edge issue_edit
type: iteration
status: in-progress
author: jkaloger
date: 2026-08-12
tags: []
related:
- implements: BUG-015
---

## Objective

close toctou window in `merge_relation_to_remote` + `resync_after_native_edge`: re-fetch remote body right before their `issue_edit`, abort/retry if changed since the read the merge built from. shrink race to network-latency scale.

## Context

- Bug: BUG-015. both fn read remote via `issue_view`, merge locally, `issue_edit` whole body back -> no re-check between read and write -> concurrent write (another session's content push, or ANY other remote body change) in that window get silently discarded when this write lands after.
- touch: `src/engine/store_dispatch.rs` -- `merge_relation_to_remote` (~876), `resync_after_native_edge` (~826). both already skip `check_lock` (975) on purpose (avoid false-reject on unrelated remote bump); keep that -- this is a narrower re-check, not the coarse timestamp lock.

## Satisfies

BUG-015 AC1-AC4.

## Tasks

1. small helper: `fn body_changed_since(remote_at_read: &str, repo, issue_number, client) -> Result<Option<GhIssue>>` -- re-`issue_view`, compare `updated_at` to what the merge was built from. `Some(fresh_issue)` if changed, `None` if same. one place, both call sites use it.
2. `merge_relation_to_remote`: after building `new_body` from the first read, before `issue_edit` -- re-check. unchanged -> push as today. changed -> retry once: re-read fresh body, re-apply the SAME relation delta (`set`/`rel_str`/`target_id`) to the fresh body, re-check again before write. still changed second time -> bail w/ clear error naming the doc, no partial write (never `issue_edit` on a body older than what merge started from).
3. `resync_after_native_edge`: same shape. native edge write (the PATCH mutation) already landed and is authoritative -- only the BODY MIRROR re-check matters here, retry re-mirrors onto fresh body, same bail-after-one-retry-fails posture.
4. no change to `update()`/`check_lock` (975) -- out of scope, tracked separately (open design note below).
5. tests, MockGhClient sequenced responses (first `issue_view` = stale, second = fresh-with-someone-else's-edit):
   - relation merge: concurrent content edit lands between read and write -> retry picks it up, both the concurrent edit's prose AND this op's relation delta present in final pushed body.
   - relation merge: TWO consecutive changes (never stabilizes) -> bails, no `issue_edit` call recorded on mock.
   - native edge resync: same concurrent-edit-survives shape.
   - dedup/no-op paths (`merge_relation_to_remote_dedup_no_edit`, 5899) still short-circuit before any `issue_view`-based re-check -- unaffected.
   - existing `merge_relation_to_remote_no_lock_preserves_prose` (5844) and `resync_after_native_edge_ignores_updated_at_and_records_fresh` (4864) stay green.

## Out of scope

- true elimination of the race (github issue api has no conditional/if-match update -- can't close the window, only shrink it).
- race against a human editing the issue in github's web ui at the exact instant of the re-check (unfixable client-side).
- `update()`/`check_lock` design. open question flagged in BUG-015: `update()` (store_dispatch.rs:1796) pulls `board_owned_status` (issue_cache.rs:874) from local disk cache mid-push. separate concern, not this iteration's scope -- don't touch.
- per-doc file lock / cross-process coordination. rejected alternative, see BUG-015 fix direction.

## Principles/conventions

CONVENTION.md 4 (fake at `GhGraphql`/`GhClient` seam, no real network in test). 6 (one shared re-check helper for both call sites, not two copies).

## Verification

- inject a body change between read and write via mock in both fn -> concurrent edit survives, this op's own delta still lands.
- inject a change that never stabilizes -> clean bail, zero `issue_edit` calls, error names the doc.
- dedup/no-op and existing lock-bypass tests unchanged.
- `cargo fmt --check && cargo clippy && cargo test`.
