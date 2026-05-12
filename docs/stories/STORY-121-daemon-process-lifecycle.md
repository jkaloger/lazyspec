---
title: Daemon process lifecycle
type: story
status: draft
author: jkaloger
date: 2026-05-12
tags: []
related:
- implements: RFC-041
- blocks: STORY-127
- blocks: STORY-128
- blocks: STORY-123
---

## In Scope

- A `lazyspec daemon` command that runs as a foreground-blocking process.
- Binding a unix domain socket at `.lazyspec/daemon.sock` on startup.
- Graceful shutdown on SIGTERM and SIGINT: drain inflight work, release any RFC-035 leases held by this host, and close/unlink the socket cleanly.
- Stale socket detection and cleanup on startup when no live daemon owns it.
- Single-instance enforcement on a given socket path.
- User-guide documentation including a sample systemd unit and a sample launchd plist for running the daemon under common supervisors.

## Out of Scope

- Assignee eligibility evaluation and the `daemon assign` flow (slice 1).
- AgentRunner, worktree creation, and Claude hooks (slice 3).
- Tick loop and agent spawning (slice 4).
- Prompt rendering for spawned agents (slice 5).
- IPC message protocol, handler dispatch, and `daemon status` CLI (slice 6); this slice only binds the socket.
- TUI consumer of daemon state (slice 7).
- Metadata refs push (slice 8).

## Acceptance Criteria

1. **Given** a workspace with no running daemon, **when** the operator runs `lazyspec daemon`, **then** the process runs in the foreground and does not return control to the shell until it receives a termination signal.
2. **Given** a running daemon, **when** the process receives SIGTERM, **then** it drains any inflight work, releases all RFC-035 leases owned by this host, closes and unlinks the socket, and exits cleanly with a success status.
3. **Given** a running daemon, **when** the process receives SIGINT (Ctrl-C), **then** it follows the same graceful shutdown path as SIGTERM.
4. **Given** the daemon has started successfully, **when** an observer inspects the workspace, **then** a unix domain socket is present at `.lazyspec/daemon.sock` and the daemon is listening on it.
5. **Given** a stale `.lazyspec/daemon.sock` left behind by a crashed previous run with no live owner, **when** `lazyspec daemon` starts, **then** it detects the stale socket, removes it, and binds a fresh socket without operator intervention.
6. **Given** a daemon is already running and listening on `.lazyspec/daemon.sock`, **when** a second `lazyspec daemon` is invoked in the same workspace, **then** the second invocation refuses to start, exits with a non-zero status, and the existing daemon is left untouched.
7. **Given** the daemon process is running, **when** the operator inspects process state, **then** the daemon has not forked into the background and has not written a PID file anywhere on disk; supervision is delegated entirely to the invoking process manager.
8. **Given** the lazyspec user guide, **when** an operator looks for production deployment guidance, **then** the guide contains a working sample systemd unit and a working sample launchd plist that invoke `lazyspec daemon` under their respective supervisors.

## Notes

- This slice deliberately stops at socket bind; protocol handlers and the `daemon status` CLI surface arrive in slice 6.
- Lease release on shutdown is scoped to leases owned by the current host as defined by RFC-035; cross-host lease cleanup is not in scope.
- "Drain inflight" here means whatever the daemon is currently doing at signal time; since spawning and tick loops land in later slices, in this slice the drain path is effectively a no-op shutdown hook that later slices hang work onto.
- Single-instance enforcement is per-socket-path, which in practice means per-workspace.

