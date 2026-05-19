---
title: "agent worktree cwd fix and continuation behavior doc"
type: iteration
status: accepted
author: "agent"
date: 2026-05-19
tags: [daemon, orchestration, bugfix]
related:
  - target: RFC-041
    type: related-to
---

## Context

Standalone bugfix. No parent Story.

Two issues observed dogfooding agent orchestration:

1. Agent subprocess runs in daemon cwd, not provisioned worktree. `claudep::spawn` builds `Command` without `.current_dir()`. `ctx.workspace` plumbed in but unused. Agent commits land on daemon's branch instead of worktree branch; `git status` / `lazyspec` calls see wrong tree.
2. After CleanExit, daemon enqueues continuation up to `max_turns` regardless of whether agent created descendant doc (e.g. iter from story). Agent that creates iter keeps looping same session and starts building immediately. No descendant-doc approval gate.

Evidence:
- `src/engine/runner/claudep.rs:20-30` Command builder. No `current_dir`. `ctx.workspace: PathBuf` defined `src/engine/runner.rs:14` but never consumed.
- `src/engine/tick.rs:1064-1076` `RetryReason::CleanExit` always enqueues continuation if `attempt+1 <= max_turns`. Status reconcile (`tick.rs:1105`) only kills on external status change.
- `docs/rfcs/RFC-041-agent-orchestration-daemon-revised.md:196` documents continuation semantics but not the descendant-doc gap.

Scope of this iter: fix #1 (real bug). Document #2 as known limitation; design + fix deferred.

## Acceptance Criteria

- **AC1 worktree-cwd.** Given `AgentContext` with `workspace = /path/to/wt`. When `ClaudeP::spawn(ctx)` runs. Then spawned subprocess inherits cwd = `/path/to/wt`. Verified by unit test that spawns short-lived process (e.g. `pwd`-equivalent) and asserts reported cwd matches workspace.
- **AC2 continuation-doc.** RFC-041 amended with explicit note: CleanExit continuation is open-loop until `max_turns` or external status change. No descendant-doc approval gate. Lists this as known limitation. References this iteration in the amendment commit.

## Changes

### 1. Pass worktree as subprocess cwd

Files: `src/engine/runner/claudep.rs`.

ACs: AC1.

Detail:
- `claudep.rs:20` Command builder. Insert `.current_dir(&ctx.workspace)` before `.stdin(...)`.
- Existing `ctx.workspace` field already populated by `tick.rs` dispatch path post-worktree-provision. No plumbing change needed.

Verification:
- Add unit test in `claudep.rs` `mod tests`. Spawn `ClaudeP` with `binary = "sh"`. Override args is not possible without trait change, so test at lower level: factor `build_command(ctx) -> Command` helper (no spawn). Assert `Command::get_current_dir() == Some(ctx.workspace.as_path())`.
- Alt path if `get_current_dir` not stable: spawn `sh -c 'pwd'` directly bypassing `ClaudeP::spawn`, capture stdout, assert equals workspace path. Pick the helper-extraction path: cheaper, no process spawn, deterministic.
- `cargo test -p lazyspec engine::runner::claudep`.
- `cargo clippy -- -D warnings`.

### 2. Document continuation behavior in RFC-041

Files: `docs/rfcs/RFC-041-agent-orchestration-daemon-revised.md`.

ACs: AC2.

Detail:
- Append to `### Retry semantics` block (around line 196):
  - Subsection "Known limitation: open-loop continuation". One paragraph stating CleanExit continuation re-spawns on same doc until `max_turns` or external status change. No check for descendant docs created during session. Agent that produces a new doc (e.g. iter from story) cannot pause for human/operator review of that doc; loop continues. Future work: descendant-doc approval gate or per-type continue policy.
- Do NOT change behavior in this iter. Doc-only.

Verification:
- `lazyspec validate --json` passes.
- Read RFC-041 post-edit, confirm note exists adjacent to existing continuation paragraph.

## Test Plan

- AC1: unit test in `src/engine/runner/claudep.rs#tests`. Helper `build_command(ctx: &AgentContext) -> Command` extracted from `spawn`. Test constructs an `AgentContext` with `workspace = TempDir`, calls `build_command`, asserts `cmd.get_current_dir()` returns `Some(workspace.as_path())`. Isolated (TempDir per test), deterministic (no spawn), fast (no I/O), specific (one assertion per behavior).
  - Tradeoff: extracting `build_command` adds one helper. Alternative is end-to-end spawn-and-pwd test, which spawns a real process per run. Helper extraction is cheaper and matches existing pattern of pure-function tests in `runner/stream.rs`.
- AC2: no automated test. Doc-only change. Manual verification: grep `RFC-041` for the new subsection heading.

## Notes

- Bug #2 design space (recorded for future iter):
  - Option A: yield-if-descendant-pending. On CleanExit scan store for descendant docs created during session whose status ∉ active_statuses. If any, treat as handoff.
  - Option B: yield-on-any-doc-mutation. Track session-touched docs; pause on any mutation.
  - Option C: drop CleanExit continuation entirely. One dispatch = one turn.
  - Option D: per-doc-type continue_policy in `[orchestration]` config.
  - Picked: defer. User direction this iter: document only.
- `AgentContext.workspace` is already populated by tick dispatch (`tick.rs` worktree provisioner). No upstream change.
- Clippy: confirm `cargo clippy` after change.
