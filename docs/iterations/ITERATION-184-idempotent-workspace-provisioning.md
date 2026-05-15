---
title: "Idempotent workspace provisioning"
type: iteration
status: accepted
author: "agent"
date: 2026-05-15
tags: []
related: []
---

## In Scope

`provision_workspace` idempotent re-entry. Pre-check existing worktree path before `git worktree add`. Reuse on match (path + branch). Error w/ actionable msg on orphan dir or branch mismatch.

## Out of Scope

- Auto-cleanup orphan worktrees / dirs (daemon policy, not provisioning).
- Stale-lease recovery (RFC-035 lease layer).
- Worktree teardown lifecycle.

## Acceptance Criteria

**AC1: re-provision w/ existing matching worktree returns workspace, no git error**

Given worktree at `<workspace_root>/<claim_id>` exists, registered via `git worktree list`, on branch `<rendered_branch>`
When `provision_workspace` called for same `claim_id` + `branch`
Then returns `Workspace { path, branch }` matching existing
And no `git worktree add` invoked
And no error

**AC2: orphan dir at worktree path errors w/ guidance**

Given `<workspace_root>/<claim_id>` exists as directory, not registered in `git worktree list`
When `provision_workspace` called
Then error names path + instructs operator to remove or run `git worktree prune`

**AC3: registered worktree on different branch errors**

Given worktree at `<workspace_root>/<claim_id>` registered on branch `other-branch` != requested
When `provision_workspace` called w/ requested branch
Then error names path, registered branch, requested branch

**AC4: first-claim path unchanged**

Given no dir, no registered worktree at `<workspace_root>/<claim_id>`
When `provision_workspace` called
Then existing behavior: ref-exists → reuse; ref-missing → fresh from base
And no regression vs ITERATION-175 AC7/8/9

## Test Plan

Per DICTUM-004: TempDir bare repo + working clone, real git, no mocks.

**AC1 test**: TempDir w/ first claim already provisioned (call `provision_workspace` once). Call again w/ same args. Assert: returns Ok, path/branch match, `git worktree list --porcelain` count unchanged (one entry for claim).

**AC2 test**: TempDir, `fs::create_dir_all` at workspace path, no `git worktree add`. Call `provision_workspace`. Assert: Err, msg contains path + "prune" or "remove".

**AC3 test**: TempDir, `git worktree add <path> <other-branch>`. Call `provision_workspace` w/ different branch. Assert: Err, msg names both branches.

**AC4 regression**: re-run ITERATION-175 worktree tests; no behavior change for clean paths.

Properties (DICTUM-004): isolated (own TempDir), behavioral (asserts on returned `Workspace` or error msg, not internal calls), readable (arrange-act-assert), specific (one scenario per test).

## Changes

### Task 1: Pre-check worktree state in `provision_workspace`

ACs: AC1, AC2, AC3, AC4.

Files:
- `src/engine/workspace.rs`: extend `provision_workspace` w/ pre-check before existing branch-ref logic.

Logic:
1. Resolve `worktree_path = workspace_root.join(claim_id)`.
2. Query `git -C repo_root worktree list --porcelain`. Parse for entry matching `worktree_path`.
3. Match cases:
   - Registered + branch matches requested → return `Workspace { path, branch }` w/o `git worktree add` (AC1).
   - Registered + branch differs → bail w/ msg naming path, registered branch, requested branch (AC3).
   - Not registered + path exists on disk → bail w/ msg naming path + `git worktree prune` guidance (AC2).
   - Not registered + path absent → fall through to existing logic (AC4).
4. Existing ref-exists/ref-missing logic unchanged.

Parser: porcelain format is line-records, `worktree <abs-path>` + `branch refs/heads/<name>` lines per record, blank-line separated. Match against canonicalized `worktree_path`. Branch line absent → detached HEAD (treat as mismatch for AC3).

Verify: `cargo test workspace`, `cargo clippy --all-targets -- -D warnings`.

### Task 2: Tests

ACs: AC1, AC2, AC3.

Files:
- `src/engine/workspace.rs` `#[cfg(test)] mod tests`: add three tests per Test Plan. Use existing TempDir bare-repo helper from ITERATION-175 tests (if present) or extract local helper.

Verify: `cargo test workspace`.

## Notes

- ITERATION-175 line 113 deferred this: "v1 errors out — operator removes." Now reached second use (daemon hits this on retry/restart) per dictum 6.
- Parser kept simple (line-record scan). No regex, no trait abstraction. If a second consumer of porcelain parsing appears, extract.
- Branch-mismatch is loud error not silent re-checkout: re-checkout could destroy operator-staged work in worktree.
- Out-of-scope orphan auto-cleanup: provisioning shouldn't unilaterally `rm -rf`. Daemon-level reclaim policy is a separate concern.

