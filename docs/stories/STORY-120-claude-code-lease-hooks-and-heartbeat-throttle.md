---
title: "Claude Code lease hooks and heartbeat throttle"
type: story
status: accepted
author: "jkaloger"
date: 2026-05-11
tags:
- hooks
- leasing
- claude-code
related:
- implements: docs/rfcs/RFC-035-git-ref-document-storage-with-lease-based-claiming.md
---


## Context

RFC-035 §"Claude Code Hooks" specifies session-start/post-tool-use/session-end hooks that drive lease claim, heartbeat, and release without the agent needing to know about coordination. The engine (Story 108) and storage backend (Story 109) are built. The hook layer is not.

PostToolUse fires per tool invocation. With a 60-minute default lease, that's many redundant fetch/push round-trips per agent. A `--min-interval` flag on `lazyspec heartbeat` lets the hook stay a one-liner while keeping network cost bounded.

Hook task resolution is env-only: `$ASSIGNED_TASK` is set by an orchestrator (future daemon, or manual export). When unset, the hook is a silent no-op so it is safe to install unconditionally via `lazyspec init`. Interactive sessions without an orchestrator are unaffected.

## Acceptance Criteria

- **Given** `$ASSIGNED_TASK=ITERATION-042` and `$CLAUDE_SESSION_ID=abc` are set
  **When** the `session-start` hook runs
  **Then** `lazyspec claim ITERATION-042 --agent-id abc --json` executes and exits 0 on success.

- **Given** `$ASSIGNED_TASK` is unset
  **When** any of the three hooks fires
  **Then** the hook exits 0 silently with no `lazyspec` invocation and no error output.

- **Given** `$ASSIGNED_TASK=ITERATION-042` and `$CLAUDE_SESSION_ID=abc` are set
  **When** the `post-tool-use` hook fires
  **Then** `lazyspec heartbeat ITERATION-042 --agent-id abc --min-interval <lease_duration/4> --json` executes.

- **Given** `lazyspec heartbeat ITERATION-042 --min-interval 15m` was called less than 15 minutes ago and recorded a timestamp at `.lazyspec/state/heartbeat-ITERATION-042`
  **When** `lazyspec heartbeat ITERATION-042 --min-interval 15m` is invoked again
  **Then** the command exits 0 with `{"skipped": true, "reason": "throttled"}` on the JSON path and performs no fetch, commit, or push.

- **Given** `.lazyspec/state/heartbeat-ITERATION-042` does not exist or is older than `--min-interval`
  **When** `lazyspec heartbeat ITERATION-042 --min-interval 15m` runs and the heartbeat succeeds
  **Then** the state file is written with the current timestamp.

- **Given** `--min-interval` is omitted
  **When** `lazyspec heartbeat` is called
  **Then** the heartbeat runs unconditionally (no state read, no throttle).

- **Given** `lazyspec init` runs in a project with at least one lease-using type
  **When** `.gitignore` is written
  **Then** `.lazyspec/state/` is present in the ignore list.

- **Given** `$ASSIGNED_TASK=ITERATION-042` and `$CLAUDE_SESSION_ID=abc` are set
  **When** the `session-end` hook fires
  **Then** `lazyspec release ITERATION-042 --agent-id abc --json` executes; a non-zero exit (e.g. lease not held) does not abort the session-end path.

- **Given** documentation under `docs/` and the project README
  **When** a reader looks for hook setup
  **Then** there is a documented snippet of `.claude/settings.json` covering all three hooks and the `$ASSIGNED_TASK` contract.

## Scope

### In Scope

- `lazyspec heartbeat --min-interval <duration>` flag. Reads/writes `.lazyspec/state/heartbeat-{type}-{id}` (or single file keyed by task id; pick one). Skips remote round-trip when last call was within the interval. `--json` reports skip vs run.
- `.gitignore` entry for `.lazyspec/state/` added by `lazyspec init` when any lease-using type is configured.
- Three hook scripts (shell or executable) wired in a `.claude/settings.json` snippet: `session-start`, `post-tool-use`, `session-end`. Env-only `$ASSIGNED_TASK` resolution.
- Hook scripts no-op when `$ASSIGNED_TASK` is unset. Non-zero `lazyspec` exit in `post-tool-use`/`session-end` must not crash the surrounding session (hook returns 0).
- README + hook-setup documentation covering install path, env contract, and behaviour when unset.

### Out of Scope

- Auto-detection of `$ASSIGNED_TASK` from a file (`.lazyspec/current-task`) or any non-env source.
- Daemon/orchestrator that produces `$ASSIGNED_TASK` and spawns Claude Code sessions.
- TUI lease indicators or claim affordance (separate story).
- Heartbeat outcome refactor (`Held` / `Lost` / `TransientFailure` enum) — folds in once hook usage demonstrates the need.
- Recovery probe / fetch-on-failure inside `heartbeat` (audit findings 5/6 liveness).
- `lazyspec start` interactive task-setting command.
