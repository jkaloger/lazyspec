---
title: Agent metadata refs
type: story
status: draft
author: jkaloger
date: 2026-05-12
tags: []
related:
- implements: RFC-041
- blocks: STORY-122
---

## In Scope

Per-agent-session metadata persisted in git refs at `refs/lazyspec/agents/{session-id}`, stored as a commit chain following the same pattern as RFC-035 leases. Each session's metadata is independently addressable and history-preserving.

The `AgentMetadata` schema carries the fields needed for orchestration visibility and recovery: `agent_id`, `session_id`, `doc_id`, `doc_type`, `status`, `started_at`, `last_event_at`, `tokens_in`, `tokens_out`, `turn_count`, and `error`. The `AgentStatus` enum includes a `crashed` variant that the boot orphan recovery path can write when a daemon restart detects a session whose worktree or process has gone away.

Metadata refs are pushed to the remote on a configurable interval (default 30 seconds, configured via `[orchestration] metadata_push_interval_ms`). Local writes always succeed even when the remote is unreachable; the next push interval retries. Other clones can fetch and read these refs to see live session state without going through the daemon process.

The session-start iteration-id snapshot is encoded in the ref data so that after a daemon restart the prompt rendering layer (story 5) can reconstruct `prior_iterations` deltas relative to the session's starting point.

The read path is a library-level concern and is callable from the TUI (story 7), the tick loop (story 4), and ad-hoc consumers without coupling to the running daemon.

## Out of Scope

- The tick loop that updates metadata on agent events lives in story 4; this story provides the write API it calls.
- Prompt rendering's consumption of the session-start iteration snapshot is story 5.
- The TUI's rendering of these refs is story 7; this story only guarantees the read path is callable.
- RFC-035 lease schema and lease semantics are unchanged and out of scope here.

## Acceptance Criteria

**AC1: Per-session metadata persisted as a commit chain**

Given a session with id `S1` is started by the daemon
When metadata is written for that session
Then a ref at `refs/lazyspec/agents/S1` exists in the local repo
And updating the metadata appends a new commit to that ref's chain rather than overwriting history

**AC2: AgentMetadata fields round-trip**

Given an `AgentMetadata` value with all fields populated (`agent_id`, `session_id`, `doc_id`, `doc_type`, `status`, `started_at`, `last_event_at`, `tokens_in`, `tokens_out`, `turn_count`, `error`)
When the value is written to the ref and then read back
Then every field on the read value equals the field on the original value

**AC3: Crashed status is writable by orphan recovery**

Given a session ref exists with status `running`
And the daemon restarts and discovers the session's worktree or process is gone
When the boot orphan recovery path runs
Then the session's metadata ref is updated with `status = crashed`
And the prior history of that session's ref is preserved

**AC4: Configurable push interval to remote**

Given `[orchestration] metadata_push_interval_ms` is configured (default 30000)
When the daemon is running with at least one active session
Then the metadata ref is pushed to the configured remote on that interval
And changing the configured value changes the observed push cadence

**AC5: Cross-machine visibility via fetch**

Given clone A has written metadata for session `S1` and the push has succeeded
When clone B fetches `refs/lazyspec/agents/*`
Then clone B can read the same `AgentMetadata` values for `S1` that clone A wrote

**AC6: Session-start iteration snapshot is recoverable**

Given a session was started against a doc with a known set of prior iterations
And the daemon process is restarted
When the metadata ref for that session is read
Then the session-start iteration-id snapshot is present in the read data
And it matches the snapshot taken at the original session start

**AC7: Remote unreachable does not block local writes**

Given the configured remote is unreachable
When metadata is written for an active session
Then the local ref update succeeds without error
And the push is retried on the next configured interval
And once the remote becomes reachable a subsequent interval succeeds in pushing accumulated updates

**AC8: Read path callable outside the daemon**

Given metadata refs exist in the repo
When a non-daemon consumer (e.g. TUI, CLI) calls the read API for a given session id
Then it returns the latest `AgentMetadata` for that session
And it does not require the daemon process to be running

## Notes

- Mirror the RFC-035 lease engine's commit-chain pattern; reuse helpers where reasonable rather than forking a parallel implementation.
- The ref namespace `refs/lazyspec/agents/{session-id}` is per-session; sessions are not reused across daemon restarts, so refs accumulate. A retention/GC concern exists but is out of scope for this slice.
- Push failures should be observable (logged or surfaced via metrics) but must not propagate as errors to callers of the write API.
- Story 4 (tick loop) is the primary write-path caller. Story 5 (prompts) is the primary reader of the session-start snapshot. Story 7 (TUI) is the primary reader of live status. Design the API surface with those three consumers in mind.

