---
title: "github issue body write path silently drops concurrent content edits"
type: bug
status: in-progress
author: "jkaloger"
date: 2026-08-12
tags: []
related: []
---

## Context

user edit issue 394 content. another session (concurrent lazyspec) also touch 394's relations. content edit vanish, relation change land. worse w/ multi-session use.

## Root Cause

github-issues doc frontmatter (status, relations, attrs) live inside issue `body` as html-comment block (`issue_body.rs`). `issue_edit` always replace WHOLE body -- github issue api no partial/conditional update.

two write path, different guard:

- `GithubIssuesStore::update()` (title/body/status/attrs push, store_dispatch.rs:1796) call `check_lock()` (store_dispatch.rs:975) first -- compare issue_map's stored `updated_at` vs remote's current. mismatch -> hard error "has been modified on github since your last fetch". guarded.
- `merge_relation_to_remote()` (store_dispatch.rs:876, used by link/unlink for non-native rel) and `resync_after_native_edge()` (store_dispatch.rs:826, native edge resync e.g. milestone/board membership) BOTH skip that lock on purpose (comment say "WITHOUT the optimistic body lock", "last-write-wins... never rejects" -- so unrelated remote bump like a comment don't block a relation edit). each: read remote body (`issue_view`) -> splice in just the rel/edge delta -> serialize -> `issue_edit` WHOLE body back. no re-check between read and write.

no cross-process lock either: `issue_map` plain json file, each process load own copy independent, no os file lock. one tui process serialize via `Arc<Mutex<GithubIssuesStore>>`, but separate sessions/processes share nothing.

net: content edit lands (via guarded `update()`). concurrent relation/native-edge op in ANOTHER session read body BEFORE that edit, write AFTER -- write silently revert content while correctly applying its own rel delta, because no re-check right before write. classic toctou, worse w/ more sessions racing same issue.

## Expected vs Actual

- expected: relation/native-edge push never silently discard content that landed on remote after this op's read.
- actual: it does, silently, no warning, no error.

## Repro

1. process A: `lazyspec update <doc> --body "new content"` on a github-issues doc (succeed, guarded).
2. process B (started earlier, or slow network): had already read remote body of same issue before A's push landed, now finishes a `lazyspec link <doc> <rel> <target>` (or a native-edge resync) and writes.
3. B's write lands after A's -> A's content gone from remote body, B's relation present. no error surfaced either process.

manual repro needs two real concurrent processes against one github issue (timing-dependent); root cause traced from code, not yet driven live.

## Fix Direction

see ITERATION-361: re-fetch remote body immediately before the final `issue_edit` in both unlocked paths, abort/retry if it changed since the read that built the merge. narrows the race to network-latency scale instead of eliminating it (github issue api has no conditional update to close it fully). does not fix a race against a human editing the github web ui at the same instant.

## Acceptance Criteria

- [ ] `merge_relation_to_remote` re-verifies remote body immediately before its `issue_edit` call; a body that changed since the read the merge was built from aborts/retries rather than overwriting silently.
- [ ] `resync_after_native_edge` gets the same treatment.
- [ ] test drives the race deterministically (inject a body change between the merge's read and its write via the mock client) and asserts the concurrent edit survives.
- [ ] `update()`'s existing `check_lock` behavior unchanged.
- [ ] full check green: `cargo fmt --check`, `cargo clippy`, `cargo test`.
