---
title: 'Agent runtime: ClaudeP runner, worktree, hooks'
type: story
status: accepted
author: jkaloger
date: 2026-05-12
tags: []
related:
- implements: RFC-041
- blocks: STORY-128
---

## In Scope

This slice introduces the agent runtime layer that the daemon will use to execute work against documents. It defines the `AgentRunner` abstraction at the subprocess seam, ships a first concrete implementation (`ClaudeP`) that drives `claude -p --output-format stream-json`, and parses the resulting stream into normalized agent events.

It also covers the workspace lifecycle around each claim: provisioning a per-claim git worktree from the configured base branch, reusing existing local branch refs without rewinding, and creating fresh worktrees when the local ref is gone. Branch names are rendered from a configurable template via minijinja (Jinja2 dialect, strict undefined, sandboxed) and then sanitized through `git check-ref-format --branch` before use. Template variables include `iteration_id`, `iteration_slug`, `agent_id`, `story_id`, and `date`.

Four hook points fire across the workspace lifecycle: `after_create`, `before_run`, `after_run`, and `before_remove`. Hooks execute as `bash -lc <script>` with the workspace as cwd and a documented set of environment variables (`LAZYSPEC_DOC_ID`, `LAZYSPEC_DOC_TYPE`, `LAZYSPEC_AGENT_ID`, `LAZYSPEC_BRANCH`, `LAZYSPEC_WORKSPACE`). Failure semantics differ by hook: `after_create` failure aborts workspace creation, `before_run` failure aborts the current attempt, and `after_run` / `before_remove` failures are logged but do not propagate.

Configuration lands under `[orchestration.runtime]` (`claude_binary`, `allowed_tools`, `turn_timeout_ms`) and `[orchestration.hooks]` (per-hook script and `timeout_ms`, default 60s).

## Out of Scope

- Tick loop scheduling and document polling (slice 4)
- Lease acquisition and continuation/retry policy (slice 4)
- Prompt content rendering and context assembly (slice 5)
- IPC streaming of events to clients (slice 6)
- TUI surfaces over the runner (slice 7)
- Metadata ref persistence for runs (slice 8)

## Acceptance Criteria

**AC1: Spawning an agent yields a control handle**

Given the daemon has selected an eligible document and prepared a workspace
When it asks the `AgentRunner` to spawn an agent for that workspace
Then it receives a handle exposing the subprocess pid, a receiver of agent events, and a cancel signal it can use to stop the run

**AC2: Session start surfaces as a normalized event**

Given a `ClaudeP` runner has been spawned
When the underlying `claude -p` process emits its `session_start` line on stream-json
Then a `SessionStarted` event is delivered through the handle's event channel

**AC3: Assistant text is delivered as deltas**

Given a `ClaudeP` run is in progress
When the underlying process streams assistant text chunks
Then each chunk is delivered as a `Text` delta event in order

**AC4: Tool invocations are surfaced with name, summary, and status**

Given a `ClaudeP` run is in progress
When the agent invokes a tool and the tool result arrives on the stream
Then a `ToolCall` event is delivered carrying the tool name, a short summary of the call, and its terminal status

**AC5: Turn completion reports token usage**

Given a `ClaudeP` turn has finished successfully
When the runner observes the turn-complete marker on the stream
Then a `TurnCompleted` event is delivered carrying the input and output token counts for that turn

**AC6: Subprocess exit is observable**

Given a `ClaudeP` run has terminated for any reason
When the underlying process exits
Then a `SubprocessExited` event is delivered carrying the exit status, and the event channel is closed

**AC7: First claim provisions a worktree from the configured base branch**

Given there is no existing local branch ref for the rendered branch name
When the runtime provisions a workspace for a claim
Then a new git worktree is created from the configured base branch (default `origin/main`) at a path scoped to that claim

**AC8: Existing local branch is reused without rewind**

Given a local branch ref for the rendered branch name already exists
When the runtime provisions a workspace for a claim against that branch
Then the worktree is attached to the existing ref and its commit history is left intact (no reset to base, no rewind)

**AC9: Missing local branch ref triggers fresh worktree from base**

Given the local branch ref for the rendered branch name has been deleted since the previous run
When the runtime provisions a workspace for a claim
Then a fresh worktree is created from the configured base branch, as if this were a first claim

**AC10: Branch names are templated and sanitized**

Given a branch name template is configured using placeholders like `iteration_id`, `iteration_slug`, `agent_id`, `story_id`, and `date`
When the runtime resolves the branch name for a claim
Then the template is rendered via minijinja with strict-undefined and sandboxed evaluation, the output is sanitized via `git check-ref-format --branch`, and the sanitized value is used as the worktree branch

**AC11: `after_create` runs after worktree creation and is fatal on failure**

Given an `after_create` hook is configured
When a worktree has just been created for a claim
Then the hook runs with cwd set to the workspace, and if it exits non-zero the workspace creation is aborted and no agent is spawned

**AC12: `before_run` runs before each turn and is fatal to that attempt**

Given a `before_run` hook is configured
When the runtime is about to spawn the agent for a turn
Then the hook runs with cwd set to the workspace, and if it exits non-zero that turn attempt is aborted before the agent is spawned

**AC13: `after_run` runs after the agent exits and failures are logged**

Given an `after_run` hook is configured
When the agent subprocess for a turn has exited
Then the hook runs with cwd set to the workspace, and any non-zero exit is logged but does not propagate as a failure

**AC14: `before_remove` runs prior to teardown and failures are logged**

Given a `before_remove` hook is configured
When the runtime is about to tear down the workspace
Then the hook runs with cwd set to the workspace, and any non-zero exit is logged but does not block teardown

**AC15: Hooks receive the documented environment**

Given any lifecycle hook is configured
When the hook is invoked
Then its process environment contains `LAZYSPEC_DOC_ID`, `LAZYSPEC_DOC_TYPE`, `LAZYSPEC_AGENT_ID`, `LAZYSPEC_BRANCH`, and `LAZYSPEC_WORKSPACE`, populated for the current claim

**AC16: Hook timeouts are honored and count as failure**

Given a hook is configured with a `timeout_ms` (default 60s when unset)
When the hook script runs longer than the configured timeout
Then the hook process is terminated and the outcome is treated as a hook failure per that hook's failure semantics

## Notes

- `AgentRunner` is the only public-facing trait name expected to appear in user docs and tests; concrete implementations (starting with `ClaudeP`) remain internal.
- Stream parsing follows the `claude -p --output-format stream-json` schema; unknown record types are tolerated and ignored to keep the runner forward-compatible with newer Claude CLI releases.
- Hooks are intentionally minimal: `bash -lc <script>` with workspace cwd. Anything more elaborate (multi-step pipelines, retries) is the script author's responsibility.
- Branch templating uses minijinja with strict-undefined so that a missing variable is a hard error at render time rather than producing a silently-malformed branch name.
- Worktree reuse policy is deliberate: we never rewind a branch the user (or a prior run) has work on. If a branch needs to start fresh, the operator deletes the local ref.
- Configuration shape:
  - `[orchestration.runtime] claude_binary`, `allowed_tools`, `turn_timeout_ms`
  - `[orchestration.hooks] after_create`, `before_run`, `after_run`, `before_remove`, each with `script` and `timeout_ms`
- Out-of-scope items belong to later slices and should not leak into this story's implementation or tests.

