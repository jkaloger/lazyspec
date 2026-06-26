---
title: "Reconcile native-relation writes with the issue-body conflict guard"
type: iteration
status: complete
author: jkaloger
date: 2026-06-26
tags: []
related:
- implements: STORY-167
---
## Changes

### 1. New conflict-free body-resync entry point on the issue store (`src/engine/store_dispatch.rs`)
- ROOT CAUSE: `link`/`unlink` of a native relation rewrites the source-issue cache `related`, PATCHes the native edge last-write-wins, then `push_if_github_backed` (`src/cli/link.rs:105,335`) -> `GithubIssuesStore::push_cache` (`:166`) -> `check_lock` (`:203`). `check_lock` (`:222`) bails `"{} has been modified on GitHub since your last fetch."` (`:224`) when `remote_issue.updated_at != local_updated_at`. An out-of-band comment bumps remote `updated_at` -> guard fires AFTER the field PATCH already applied -> half-applied (remote has edge, the `push_cache` body resync + `updated_at` reconcile never run, whole `link_inner` returns `Err`).
- `push_cache` (`:166`) re-serializes cache `meta`/`body` -> `issue_edit` (`:177`) -> re-mirrors the SAME body it just read; the body conflict guard adds NO protection here because the body is not the thing the native-relation write changed (the `related` frontmatter is NOT round-tripped into the GitHub body anyway -- `issue_body::serialize` carries author/date/body, not `related`). So gating this resync on body staleness is pure false-positive for native-relation writes.
- ADD `pub fn resync_after_native_edge(&mut self, type_def: &TypeDef, doc_id: &str) -> Result<()>` next to `push_cache` (`:166`). Body of `push_cache` MINUS the lock-bail branch:
  - read cache file (`find_cache_file` `:168`), `DocMeta::parse` + `DocMeta::extract_body` (`:171`).
  - resolve `issue_number` from `self.issue_map.get(doc_id)` directly (NOT via `check_lock`).
  - `issue_view` the remote ONCE to capture its CURRENT `updated_at` (last-write-wins: remote is authoritative for the timestamp; we never reject on it).
  - `issue_body::serialize(&meta,&body)` -> `issue_edit(..)` (`:177` shape unchanged) to re-mirror the body. This is the post-PATCH known-good push -> no conflict possible.
  - re-fetch (or reuse the freshly-returned) remote `updated_at` and store it: `self.issue_map.insert(doc_id, issue_number, &remote_updated_at, node_id)` so the next ordinary body write has an accurate lock baseline (do NOT leave it empty as `push_cache` does, to keep the invariant tight). `issue_map.save`; `issue_cache.touch_lock(doc_id)` (`:183`).
- LEAVE `push_cache` and `check_lock` UNCHANGED -> ordinary `update`/`set_provenance`/`delete` body writes (`:776,899,942`) keep the optimistic lock (AC3).

### 2. Route native-relation cache mirror through the conflict-free resync (`src/cli/link.rs`)
- `push_if_github_backed` (`:344`) is the cache-mirror hop for BOTH ordinary links and native-relation links on a github-issues source. It ALWAYS calls `gh_store.push_cache(type_def,&doc_id)` (`:406`) -> body conflict guard. Native milestone/membership writes flow through the SAME hop because the source doc (e.g. `STORY-N`) is github-issues backed.
- THREAD a `native_edge: bool` into the mirror so the native path skips the lock. `link_inner` (`:60`) already knows: `apply_native_milestone` (`:87`) / `apply_native_membership` (`:96`) each test `github_native == Some("milestone"|"membership")`. CHANGE those two fns to RETURN `Result<bool>` (true when they actually performed a native write, false on the ordinary no-op early-return `:132,184`). In `link_inner`/`unlink_inner` (`:87-104`,`:317-334`): `let native = apply_native_milestone(..)? || apply_native_membership(..)?;` then `push_if_github_backed(root,&resolved_from,Some(config),client_factory, native)?`.
- `push_if_github_backed` (`:344`) signature gains `native_edge: bool`. At the dispatch (`:406`): `if native_edge { gh_store.resync_after_native_edge(type_def,&doc_id) } else { gh_store.push_cache(type_def,&doc_id) }`. Ordinary `implements`-style links keep `push_cache` + lock (AC3 path untouched).
- `push_if_git_ref_backed` (`:409`) UNCHANGED -> git-ref relations never hit the issue conflict guard.

### 3. Symmetric on unlink (`src/cli/link.rs`)
- `unlink_inner` (`:280`) is structurally identical: frontmatter `retain` (`:303`) drops the relation, native unlink PATCHes milestone `None` / `deleteProjectV2Item`, then the SAME `push_if_github_backed` hop (`:335`). Apply the identical `native` plumbing so an out-of-band-edit unlink mirrors without the guard (AC4).

## Test Plan

- AC1 (out-of-band comment does NOT block native milestone link): `link_inner` test like `link_native_milestone_sets_and_clears_association` (`src/cli/link.rs:531`) but seed `issue_map` STORY-7 with a NON-empty stale `updated_at` (e.g. `"2026-06-26T10:00:00Z"`) and a `MockGhClient::with_view_issue` whose `updated_at` is LATER (`"...T11:00:00Z"`, simulating a comment bump). Assert `link_inner(...)` returns `Ok` (no `"modified on GitHub since your last fetch"`), `recorder.last_set_milestone == Some((7,Some(3)))`, and the STORY-7 cache `.md` contains `targets: MILESTONE-3` -> remote edge + cache agree.
- AC2 (no half-applied state, milestone + membership): assert post-`link_inner` STORY-7 cache `related` carries the edge AND the native write recorded (milestone: `last_set_milestone`; membership: one `addProjectV2ItemById`). Re-`IssueMap::load` -> STORY-7 `updated_at` == the remote's fresh timestamp (resync recorded it), not empty/stale. Fake at the gh seam = `MockGhClient` (graphql responses for membership, `with_view_issue` for milestone source resync) + `MockGhMilestoneClient`.
- AC3 (body conflict guard still fires for ordinary body writes): existing `GithubIssuesStore::update` test path -- seed stale `updated_at`, remote `updated_at` advanced, call `update(&td,doc_id,&[("status","accepted")])` (an ordinary, non-native body write) -> still `Err` containing `"modified on GitHub since your last fetch"`. `resync_after_native_edge` is NOT on this path; `check_lock` unchanged.
- AC4 (unlink symmetric under out-of-band edit): `unlink_inner` with stale-then-advanced `updated_at`; for milestone assert `Ok` + `last_set_milestone == Some((7,None))` + cache `related` no longer has `targets: MILESTONE-3`; for membership reuse `unlink_membership_removes_only_that_board` (`:807`) seeding the advanced timestamp -> `deleteProjectV2Item` recorded, cache mirror completes, surviving membership stays.
- Regression: `link_native_milestone_sets_and_clears_association`, `link_membership_adds_project_item` (`:649`), `link_membership_two_boards_two_adds` (`:716`), `link_with_config_triggers_github_push_for_cached_doc` (`:948`, ordinary `implements` link still uses `push_cache` + clears `updated_at`) all still pass after the `apply_native_*` return-type and `push_if_github_backed` signature changes.

## Notes

- INVARIANT restored: after a successful native `link`/`unlink`, cache `.related` matches the remote edge regardless of unrelated remote `updated_at` advances. The native field PATCH is authoritative (last-write-wins, STORY-158/STORY-161); the body resync is bookkeeping, not a contended write -> it must not be gated by body staleness.
- WHY a separate fn not a flag inside `check_lock`: `check_lock` returns the fetched `remote_issue` that `update`/`set_provenance`/`delete` deserialize for the body round-trip; weakening it would silently drop body protection for those. The native path needs no deserialize-remote step (it re-mirrors the local cache body it just wrote) -> a distinct `resync_after_native_edge` keeps the two policies cleanly separate.
- `related` frontmatter is NOT serialized into the GitHub issue body (`issue_body::serialize` emits author/date + body only) -> the native edge of record is the milestone field / Projects v2 item, and the cache `related` is the LOCAL mirror; the body `issue_edit` in resync is effectively a no-op re-push whose only job is to refresh `updated_at` cleanly. We could skip the `issue_edit` entirely, but keeping it preserves parity with `push_cache` (any author/date drift still flushes) at zero extra risk since no conflict gate applies.
- `apply_native_*` returning `bool` is load-bearing: the ordinary-relationship early-returns (`:132`,`:184`) MUST yield `false` so plain `implements`/`blocks` links keep the `push_cache` lock; only a genuine `github_native` write flips `native_edge` true.
- Scope guard (per STORY-167 Out of Scope): NO field-level conflict detection, NO three-way merge, NO weakening of body-write protection. Last-write-wins retained for native fields; optimistic lock retained for the issue body. STORY-163/165 schema-snapshot/org-resolution warnings untouched.
- `--json` on `show`/`status` unchanged; the fix is entirely in the write path. Verify after: `show STORY-N --json` `.related` reflects the edge.
