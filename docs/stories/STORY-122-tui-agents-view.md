---
title: TUI agents view
type: story
status: draft
author: jkaloger
date: 2026-05-12
tags: []
related:
- implements: RFC-041
---

## In Scope

A new TUI view, `agents`, surfaces live state of the daemon-managed agent fleet. The view is a two-panel layout: an agent list on the left, and a live output stream on the right for the selected agent. The list renders one row per agent with a status icon, the doc id being worked on, elapsed wall-clock time, and token totals (input/output). Selecting a row binds the right panel to that agent's live output stream.

The view consumes daemon socket events via a subscription to render incremental updates without polling. A status bar at the bottom shows daemon connection state, aggregate agent counts (by status), and total tokens across all agents.

A manual-kickoff hotkey opens a document picker; selecting a document performs the same store write as `lazyspec assign` (frontmatter mutation adding the configured agent user) and, when the daemon is reachable, sends a `kick` message over the socket so the agent can start immediately.

When the daemon is offline, the view falls back to reading `refs/lazyspec/agents/*` and renders a read-only history of past sessions. The offline state is clearly indicated in the status bar and chrome so users do not mistake stale data for live state.

Transient socket loss is handled by automatic reconnect — the view does not crash and resumes streaming once the daemon is reachable again.

## Out of Scope

- The IPC protocol itself — defined in slice 6.
- Daemon lifecycle (start, foreground blocking, shutdown) — slice 2.
- The assign frontmatter schema and write path — slice 1. The TUI calls into the established write path rather than redefining it.
- Metadata ref schema for `refs/lazyspec/agents/*` — slice 8. The TUI reads via the established schema.
- The daemon tick loop driving agent state transitions — slice 4.

## Acceptance Criteria

**AC1: two-panel layout**

Given the user has opened the `agents` view
When the view renders
Then the left panel shows an agent list and the right panel shows a live output stream area

**AC2: agent list columns**

Given one or more agents are tracked
When the agent list renders
Then each row shows a status icon, the doc id being worked on, elapsed time, and token totals

**AC3: streaming selected agent**

Given the user selects an agent in the list
When the daemon emits output for that agent
Then the right panel renders the new output incrementally without requiring a refresh

**AC4: status bar**

Given the view is open
When the daemon emits state updates
Then the status bar reflects current daemon connection state, agent counts by status, and total tokens

**AC5: manual kickoff**

Given the user presses the manual-kickoff hotkey
When the user selects a document from the picker
Then the document's frontmatter is updated to include the configured agent user, and a kick is sent to the daemon when reachable

**AC6: offline fallback**

Given the daemon is not reachable
When the user opens the agents view
Then the view renders historical sessions from `refs/lazyspec/agents/*` in a read-only mode with an offline indicator visible

**AC7: reconnect on transient loss**

Given the view is streaming live updates
When the socket connection drops momentarily
Then the view does not crash, reconnects automatically, and resumes streaming once the daemon is reachable

## Notes

Per Dictum 3, the TUI depends on the engine only — kickoff writes go through the engine's assign path directly, not through the CLI. ACs above are phrased around observable behavior rather than implementation layering, but the implementation must respect this boundary.

The status icon vocabulary should align with the daemon's agent state machine defined in slice 4; reuse the same status names where possible so users see consistent terminology across CLI, daemon logs, and TUI.

