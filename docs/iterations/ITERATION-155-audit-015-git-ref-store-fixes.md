---
title: audit-015-git-ref-store-fixes
type: iteration
status: accepted
author: jkaloger
date: 2026-04-06
tags: []
related:
- related-to: AUDIT-015
- related-to: STORY-108
- related-to: STORY-109
---





## Context

AUDIT-015 ran 30 manual tests against the git-ref storage backend and lease engine. 7 issues were found, ranging from high (coordination workflow blockers) to low (missing flags). This iteration fixes all 7.

## Changes

### Task 1: Add parent support to `create_commit` (Findings B, C)

**Fixes:** Finding B (heartbeat CAS failure), Finding C (orphan commits)

**Files:**
- `src/engine/git_ref.rs` (trait definition, `GitCli` impl, `MockGitRefClient`)

**Implementation:**

1. Add an `Option<&str>` parent parameter to `create_commit` in the `GitRefOps` trait (line 9). Signature becomes: `fn create_commit(&self, root: &Path, refname: &str, files: &[(&str, &str)], parent: Option<&str>) -> Result<String>`
2. In `GitCli::create_commit` (line 135), when `parent` is `Some(sha)`, pass `-p <sha>` to `git commit-tree`. When `None`, omit it (current behavior, creates orphan).
3. `create_ref_commit` (line 151) continues to call `create_commit` with `parent: None` since it creates initial refs.
4. Update `MockGitRefClient::create_commit` (line 340) to accept the new parameter and record it in calls.
5. Update all existing callers of `create_commit` to pass the appropriate parent:
   - `GitRefStore::update` (git_ref_store.rs:193): pass `Some(&old_sha)` so updates form commit chains.
   - `LeaseEngine::heartbeat` (lease.rs:168): change from `create_ref_commit` to `create_commit` with `parent: Some(&old_sha)`, then `update_ref` with CAS. This fixes the double-update-ref that causes the CAS failure.

**Verification:** `cargo test`. Heartbeat unit tests should pass with CAS. New test for commit chain parentage (see Test Plan).

### Task 2: Stabilize agent ID fallback (Finding A)

**Fixes:** Finding A (non-deterministic agent ID)

**Files:**
- `src/engine/agent.rs`

**Implementation:**

Drop the PID component from the git-config fallback. Change line 45 from `Ok(format!("{}-{}", user_name, encoded))` to just `Ok(user_name)`. Remove the sqids PID encoding (lines 41-44). The `sqids` import can stay if other modules use it; otherwise remove from this file.

Rationale: the PID suffix was meant to distinguish multiple terminals by the same user, but it breaks the core use case (lease continuity across CLI invocations). Users who need per-terminal isolation should set `$LAZYSPEC_AGENT_ID`. The `$CLAUDE_SESSION_ID` path already handles agent sessions.

**Verification:** Existing unit tests updated: `falls_back_to_git_config_with_sqids_pid` renamed and updated to assert exact `user_name` without suffix. `both_empty_strings_falls_back_to_git` updated similarly.

### Task 3: Wire `force_acquire` to CLI (Finding E)

**Fixes:** Finding E (force_acquire unreachable from CLI)

**Files:**
- `src/cli.rs` (Claim command definition)
- `src/cli/lease.rs` (`run_claim` function)

**Implementation:**

1. Add `#[arg(long)] force: bool` to the `Claim` variant in `src/cli.rs` (around line 250).
2. In `run_claim` (lease.rs:118), when `force` is true, call `engine.force_acquire(root, type_name, id, &agent, now)` instead of `engine.acquire(...)`.
3. Pass `force` from `main.rs` claim handler to `run_claim`.

**Verification:** Manual test: claim with short lease, wait for expiry, `claim --force` succeeds. Unit test: mock-based test that `force_acquire` is called when flag is set.

### Task 4: Route `link`/`unlink` through GitRefStore for git-ref documents (Finding D)

**Fixes:** Finding D (link writes to cache, not git ref)

**Files:**
- `src/cli/link.rs`

**Implementation:**

In `link_inner` (line 46) and `unlink_with_config` (line 81), after `rewrite_frontmatter` modifies the cache file, add a `push_if_git_ref_backed` step that mirrors the existing `push_if_github_backed` pattern:

1. Detect if the document's type has `StoreBackend::GitRef` (same path-based detection as `push_if_github_backed`, line 123-144).
2. If git-ref backed: read the updated cache file content, create a new commit via `GitCli.create_commit` with the current ref SHA as parent, then `update_ref` with CAS, then update `cache.lock` with the new SHA.
3. Apply to both `link_inner` and `unlink_with_config`.

**Verification:** Integration test: create git-ref doc, link it, verify the git ref blob contains the relationship. Verify fetch after link preserves the relationship.

### Task 5: Add `--json` to `update` command (Finding F)

**Fixes:** Finding F (Principle 2 violation)

**Files:**
- `src/cli.rs` (Update command definition)
- `src/main.rs` (update handler)
- `src/cli/update.rs`

**Implementation:**

1. Add `#[arg(long)] json: bool` to the `Update` variant in `src/cli.rs`.
2. In the update handler in `main.rs`, when `json` is true, after `run_with_config` succeeds, load the updated document via `Store::load` and output as JSON using `doc_to_json` (same pattern as `create`'s JSON path in `create.rs:99-117`).
3. Pass `json` through to `main.rs` handler.

**Verification:** `cargo run -- update <doc> --status draft --json` produces JSON output with updated fields.

### Task 6: Integrate git-ref numbering with reservation system (Finding G)

**Fixes:** Finding G (number reuse after delete)

**Files:**
- `src/engine/git_ref_store.rs` (`next_number_from_refs`, `create`)

**Implementation:**

Replace `next_number_from_refs` with a call to the existing reservation system. In `GitRefStore::create` (line 96), instead of calling `self.next_number_from_refs(type_def)`, call `reservation::reserve_next(root, &type_def.prefix, remote, on_progress)` where `remote` comes from `self.config.coordination` (or defaults to `"origin"`).

This gives git-ref types the same atomic, collision-free numbering that filesystem types use. The reservation ref (`refs/reservations/{PREFIX}/{N}`) is a different namespace from the document ref (`refs/lazyspec/{type}/{id}`), so they don't conflict.

If `coordination` config is absent, fall back to `next_number_from_refs` (no remote available for reservation push). Add a `remote` field to `GitRefStore` or read it from config.

Propagate the `on_progress` callback through `DocumentStore::create` or add a default no-op. Check if `DocumentStore::create` signature needs updating. If the trait change is too invasive, an alternative is to call `reserve_next` in the CLI `create.rs` before calling `GitRefStore::create`, passing the reserved number. This mirrors how `fs_ops::create_document` already receives the number from the reservation system.

**Verification:** Integration test: create, delete, create again. Second create gets N+2 (not N, the deleted number). Verify reservation ref exists on local refs.

## Test Plan

### Unit tests

| Test | Verifies | Properties |
|------|----------|------------|
| `create_commit_with_parent_chains` | Task 1: `create_commit` with `Some(parent)` records `-p` in mock calls | Isolated, behavioral, specific |
| `create_commit_without_parent_orphans` | Task 1: `create_commit` with `None` omits parent | Isolated, behavioral |
| `heartbeat_uses_create_commit_then_cas` | Task 1: heartbeat calls `create_commit` (not `create_ref_commit`) then `update_ref` with correct old/new SHAs | Isolated, behavioral, specific |
| `agent_fallback_uses_git_username_only` | Task 2: fallback is just `git config user.name`, no PID suffix | Isolated, deterministic |
| `run_claim_with_force_calls_force_acquire` | Task 3: `--force` flag dispatches to `force_acquire` | Isolated, behavioral |

### Integration tests

| Test | Verifies | Properties |
|------|----------|------------|
| `update_creates_chained_commit` | Task 1: after update, `git cat-file -p <sha>` shows parent matching previous SHA | Behavioral, specific |
| `heartbeat_succeeds_and_extends_expiry` | Task 1: heartbeat on a claimed doc succeeds, new expiry > old expiry | Behavioral, specific |
| `link_git_ref_doc_persists_to_ref` | Task 4: link a git-ref doc, verify the ref blob contains the relationship | Behavioral, specific |
| `link_git_ref_doc_survives_fetch` | Task 4: link, wipe cache, list -- relationship still present | Behavioral, structure-insensitive |
| `update_with_json_flag` | Task 5: `update --json` returns valid JSON with updated fields | Behavioral, readable |
| `create_after_delete_skips_deleted_number` | Task 6: create N, delete N, create again -> gets N+1 (not N) | Behavioral, deterministic |

### Test tradeoffs

- Task 1 (heartbeat) could be tested purely via mocks or via integration against a real git repo. Mock tests verify the call sequence (which matters for the CAS bug). Integration tests verify end-to-end correctness. Both are needed.
- Task 4 (link persistence) requires testing with real git operations to verify the ref is actually updated. Mock-only testing would miss the same class of bug that caused Finding D.
- Task 6 (reservation integration) depends on whether we call `reserve_next` in the engine or the CLI. If engine: mock the reservation. If CLI: integration test with tempdir git repo.

## Notes

- Tasks 1 and 2 are the highest priority (unblock coordination workflow).
- Task 1 touches the `GitRefOps` trait signature, which affects all implementors (GitCli, MockGitRefClient) and all callers. The change is mechanical but wide.
- Task 6 has a design choice: integrate reservation into `DocumentStore::create` vs. keep it in the CLI layer. The CLI-layer approach avoids changing the trait but duplicates the reservation call pattern from `fs_ops::create_document`. Recommend CLI-layer for now (matches existing pattern, Principle 6: no indirection until two uses).
