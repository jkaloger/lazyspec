---
title: Orchestrator tick loop
type: story
status: accepted
author: jkaloger
date: 2026-05-12
tags: []
related:
- implements: RFC-041
- blocks: STORY-125
- blocks: STORY-122
- blocks: STORY-124
---

Slice 4 of RFC-041. Implements the orchestrator tick loop that runs inside `lazyspec daemon`: poll documents, filter by eligibility, acquire RFC-035 leases, dispatch up to a configured concurrency, heartbeat while agents run, and reconcile each tick against doc status, stall timers, turn timeouts, and exit signals. Tick loop calls into the AgentRunner/worktree machinery from slice 3 and the prompt renderer from slice 5; it does not own those components. It does not handle IPC, metadata refs, or the daemon process lifecycle.

The tick is the heartbeat of the daemon. Everything observable about agent progress (continuation, retry, stall, handoff, termination) flows through the per-tick reconciliation pass. Behaviour is deterministic against configuration: `poll_interval_ms`, `max_concurrent_agents`, `stall_timeout_ms`, `turn_timeout_ms`, `max_turns`, `max_failure_attempts`, `max_retry_backoff_ms`, `active_statuses`, `agent_users`, `claim_type`.

## In Scope

- Polling loop on `[orchestration] poll_interval_ms` (default 30s).
- Candidate fetch via the Store, sliced by `claim_type` (default `story`).
- Eligibility filter: doc status is in `active_statuses`, assignees intersect `agent_users`, no active local lease ref already held for the doc.
- Candidate ordering: priority ascending, created_at ascending, id ascending.
- Dispatch loop bounded by `max_concurrent_agents`.
- RFC-035 lease acquire/heartbeat/release; daemon holds the lease, not the spawned agent. Heartbeat default 5 minutes.
- Lease agent identifier shaped as `{host}:{session_id}`. Host is a stable per-machine id derived from gethostname plus a daemon-local UUID persisted at `.lazyspec/daemon-host-id`. Session id is unique per agent session.
- Batched `git fetch refs/lazyspec/leases/*` piggybacked on the metadata-push interval (default 30s) rather than fetched per tick.
- Acquire-time CAS against the all-zeros SHA as a safety net for a stale local view.
- Per-tick reconciliation across all running agents covering stall detection, turn timeout, status refresh, continuation, and failure backoff.
- Stall detection: no stream-json events received for `stall_timeout_ms` (default 5 minutes) results in SIGTERM and a retry queue entry. Stall timer is suspended while a tool_use is in flight.
- Hard per-turn wall via `turn_timeout_ms` (default 1 hour).
- Status refresh: re-load each running doc each tick. Terminal status kills the agent, releases the lease, and removes the worktree. Handoff status kills the agent, releases the lease, and keeps the worktree. Active status continues the run.
- Continuation on clean exit with status still active: 1 second delay, fresh `claude -p` invocation in the same workspace, increment attempt, cap `max_turns = 20`.
- Failure backoff on abnormal exit, stall, or hook failure: exponential `min(10000 * 2^(n-1), max_retry_backoff_ms)`, cap `max_failure_attempts = 5`.
- Post-cap behaviour: release lease, emit a `failed` agent event, do not mutate doc status.
- Boot orphan recovery: leases prefixed with this host id wait one RFC-035 lease grace_period, then admin-release; worktree is left in place; the agent ref `refs/lazyspec/agents/{session-id}` is marked `crashed`.
- Preflight at daemon start: workflow file is readable, prompt template renders, `agent_users` is non-empty. Preflight is invalidated and re-run when notify events fire on the config or prompt files.

## Out of Scope

- AgentRunner trait, worktree creation, and Claude Code hooks (slice 3 — tick loop calls into these).
- Prompt rendering itself (slice 5 — tick loop calls the renderer).
- IPC message handling (slice 6 — tick loop emits events but does not service the socket).
- Metadata ref schema and push cadence (slice 8 — tick loop writes through that path).
- Daemon process lifecycle, signal handling, and supervision (slice 2).
- `lazyspec assign` CLI and the `agent_users` config schema (slice 1).

## Acceptance Criteria

**AC1 — Polling cadence**
Given the daemon is running with `poll_interval_ms = 30000`
When the daemon has been up for one minute
Then the tick loop has fired approximately twice and each tick has loaded the candidate set from the configured store.

**AC2 — Eligibility filter**
Given a candidate doc has status in `active_statuses`, has at least one assignee in `agent_users`, and has no active local lease ref
When the tick evaluates candidates
Then the doc is selected for dispatch; if any of those three conditions is false the doc is skipped.

**AC3 — Concurrency cap**
Given `max_concurrent_agents = 3` and five eligible candidates
When the dispatch loop runs on a single tick
Then exactly three agents are dispatched and the remaining two remain in the candidate queue for the next tick.

**AC4 — Lease acquire before spawn with CAS**
Given a candidate has been selected
When the daemon dispatches the agent
Then a lease acquire is attempted with a CAS against the all-zeros SHA before the agent process is spawned, and a CAS failure causes the candidate to be skipped on this tick without spawning.

**AC5 — Heartbeat cadence**
Given the daemon holds a lease for a running agent and the heartbeat interval default is 5 minutes
When the agent has been running for more than one heartbeat interval
Then the daemon (not the agent) has issued at least one heartbeat write against the lease.

**AC6 — Lease agent identifier shape**
Given the daemon starts on a host without an existing host id file
When the daemon acquires its first lease
Then a UUID has been persisted at `.lazyspec/daemon-host-id`, and the lease `agent` field has the form `{host}:{session_id}` where host is stable across daemon restarts on the same machine.

**AC7 — Batched lease fetch**
Given the daemon polls every 30s and the metadata-push interval is 30s
When the daemon runs for several ticks
Then `git fetch refs/lazyspec/leases/*` is issued on the metadata-push cadence rather than once per tick.

**AC8 — Stall detection with tool_use suspension**
Given an agent has produced no stream-json events for `stall_timeout_ms` and no tool_use is in flight
When the reconcile step runs
Then the agent is sent SIGTERM and queued for retry. Given a tool_use was in flight throughout that window the stall timer is suspended and no SIGTERM is sent.

**AC9 — Turn timeout**
Given an agent's current turn has been running for `turn_timeout_ms`
When the reconcile step runs
Then the turn is terminated and treated as an abnormal exit for retry purposes.

**AC10 — Terminal status reconcile**
Given a running agent's doc transitions to a status not in `active_statuses` and not a handoff status
When the next tick reconciles
Then the daemon kills the agent, releases the lease, and removes the worktree.

**AC11 — Handoff status reconcile**
Given a running agent's doc transitions to a handoff status
When the next tick reconciles
Then the daemon kills the agent, releases the lease, and leaves the worktree in place.

**AC12 — Clean exit continuation**
Given an agent exits cleanly and the doc status is still active and `attempt < max_turns`
When the daemon observes the exit
Then after a 1 second delay a fresh `claude -p` is invoked in the same workspace, attempt is incremented, and continuation halts once attempt reaches `max_turns = 20`.

**AC13 — Failure backoff**
Given an agent fails via abnormal exit, stall, or hook failure for the nth time
When the daemon schedules the retry
Then the next attempt is delayed by `min(10000 * 2^(n-1), max_retry_backoff_ms)`, capped at `max_failure_attempts = 5`.

**AC14 — Post-cap behaviour**
Given an agent has reached `max_failure_attempts`
When the daemon gives up on the candidate
Then the lease is released, a `failed` agent event is emitted, and the doc status is not mutated by the daemon.

**AC15 — Boot orphan recovery**
Given the daemon starts and finds leases prefixed with this host id from a prior crashed session
When boot recovery runs
Then the daemon waits one RFC-035 lease grace_period, admin-releases each orphan lease, leaves the corresponding worktree in place, and marks `refs/lazyspec/agents/{session-id}` as `crashed`.

**AC16 — Preflight at start and on config change**
Given the daemon is starting
When preflight runs
Then it verifies the workflow file is readable, the prompt template renders, and `agent_users` is non-empty. Given a notify event fires on the config file or the prompt file later, preflight is invalidated and re-run before the next dispatch.

## Notes

- Tick loop is the single owner of agent lifecycle observability inside the daemon. All retry, continuation, stall, and reconcile decisions are made here.
- Lease authority is RFC-035; this slice consumes that engine and does not redesign it.
- Per-tick git fetch is deliberately avoided. Lease freshness rides on the metadata-push interval so a daemon doing frequent ticks (e.g. 5s) does not hammer the remote.
- The CAS-against-zeros acquire is a safety net only; the primary defence is the local lease ref check during eligibility filtering.
- Continuation and failure are distinct paths. Continuation increments `attempt` against `max_turns`; failure increments against `max_failure_attempts`. They do not share a counter.
- The daemon never mutates doc status. All status transitions are agent-driven through normal lazyspec commands. The daemon only reacts to status it observes.
- Orphan recovery is intentionally conservative: leave worktrees alone, mark the agent ref as crashed, and let an operator decide whether to resume or discard.

