---
title: IPC protocol and daemon status CLI
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

Implement the unix-socket IPC protocol that connects the lazyspec daemon to its clients, plus the `lazyspec daemon status` CLI that consumes it. The protocol is newline-delimited JSON: every message is a single self-contained JSON object terminated by `\n`. Client→daemon messages are `subscribe`, `unsubscribe`, `cancel`, `status`, and `kick`. Daemon→client messages are `agent_event`, `agent_status`, `daemon_status`, and `error`.

The socket itself is bound and unbound by the daemon lifecycle in slice 2; this slice only handles the protocol layered on top. Many clients may connect simultaneously — the daemon fans events out to every subscribed client. `cancel` accepts either an `agent_id` or a `session_id` and sends SIGTERM to that agent's subprocess. `status` returns a one-shot snapshot of currently running agents (id, document, elapsed time, token usage). `kick` shortcuts the daemon's next poll wait so a freshly-assigned document is picked up immediately.

`lazyspec daemon status [--json]` is a thin client: it connects, sends a `status` request, prints the snapshot, and exits. It does not fork a daemon when one is absent — if the socket is missing or unreachable it exits non-zero with a clear "daemon not running" message. Subscribers (used by the TUI in slice 7) reconnect automatically after a transient disconnect, resubscribing on reconnect.

## Out of Scope

- Binding/unbinding the socket itself (slice 2 — daemon lifecycle)
- TUI consumer of the event stream (slice 7)
- Tick loop event sources (slice 4 — this slice consumes events the tick loop emits)
- Concrete `AgentRunner` event payloads (slice 3 — this slice serialises them)
- `lazyspec assign` command (slice 1 — that slice may use `kick` once this lands)
- Windows named-pipe transport (deferred beyond v1)

## Acceptance Criteria

**AC1: newline-delimited JSON framing**
- Given a client connected to the daemon socket
- When the daemon and client exchange messages
- Then every message is a single JSON object terminated by `\n` with no embedded newlines

**AC2: subscribe receives streamed events**
- Given a client has sent `subscribe`
- When the daemon emits `agent_event` or `agent_status` messages
- Then the subscribed client receives every such message until it sends `unsubscribe` or the connection drops

**AC3: fan-out to multiple subscribers**
- Given two or more clients have subscribed
- When the daemon emits an event
- Then every subscribed client receives the same event

**AC4: cancel terminates the target agent**
- Given an agent is running with a known `agent_id`
- When a client sends `cancel` for that `agent_id` (or its `session_id`)
- Then the daemon sends SIGTERM to that agent's subprocess

**AC5: status snapshot**
- Given the daemon has zero or more running agents
- When a client sends `status`
- Then the daemon responds with a single `daemon_status` message listing each running agent's id, document, elapsed time, and token usage

**AC6: kick shortcuts the poll wait**
- Given the daemon is sleeping between poll intervals
- When a client sends `kick`
- Then the daemon performs an immediate rescan instead of waiting for the next tick

**AC7: daemon status CLI prints snapshot**
- Given the daemon is running
- When the user runs `lazyspec daemon status --json`
- Then the command connects, prints the daemon snapshot as JSON, and exits zero

**AC8: daemon status CLI handles absent daemon**
- Given no daemon is running (socket missing or unreachable)
- When the user runs `lazyspec daemon status`
- Then the command does not attempt to spawn a daemon, prints a clear "daemon not running" message, and exits non-zero

**AC9: subscriber reconnects after transient disconnect**
- Given a subscribed client loses its connection
- When the daemon socket becomes reachable again
- Then the client reconnects automatically and resubscribes without caller intervention

## Notes

- Unix-only in v1; Windows named-pipe transport is deferred.
- The socket path is `.lazyspec/daemon.sock` (established in slice 2).
- Event payload shapes for `agent_event` / `agent_status` come from `AgentRunner` in slice 3 — this slice owns the framing and routing, not the payload schema.
- `cancel` must accept both `agent_id` and `session_id` to support both daemon-internal identifiers and Claude session identifiers surfaced via lease metadata.
- Errors from malformed requests are reported via the `error` message rather than closing the connection.

