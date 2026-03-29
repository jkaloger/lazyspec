---
title: Agent Orchestration Daemon
type: rfc
status: draft
author: jkaloger
date: 2026-03-27
tags:
- agents
- orchestration
- daemon
- tui
related:
- supersedes: RFC-016
- related-to: RFC-035
---




## Problem

RFC-016 sketched basic agent invocation from the TUI: press `a`, pick an action, spawn a headless Claude process. Three stories (STORY-051, STORY-052, STORY-053) implement this as a fire-and-forget model where the TUI spawns processes and polls for completion. Agent records are persisted as JSON files in `~/.lazyspec/agents/`.

RFC-035 added coordination primitives: lease-based locks, agent identity, git-ref storage, and heartbeat. These provide the building blocks for multi-agent workflows but don't address who actually _runs_ the agents.

The gap is an orchestrator. Right now there's nothing that:
- Manages multiple concurrent agents with backpressure
- Streams live output from running agents into the TUI
- Syncs agent progress to the remote so other collaborators can see what's happening
- Recovers from agent crashes (the current `poll_finished` just checks exit status)
- Assigns work automatically based on available iterations

The current `AgentSpawner` in `src/tui/agent.rs` is coupled to the TUI process. If the TUI exits, agent tracking is lost. Records are local JSON files, invisible to other clones. Output is only accessible after completion via `$EDITOR`.

## Intent

Add an orchestrator daemon (`lazyspec daemon`) that runs as a background process, manages headless Claude instances, and exposes an IPC interface for TUI and CLI consumers.

The daemon:
- Spawns and supervises Claude processes using `claude -p --output-format stream-json`
- Claims iterations on behalf of agents using RFC-035's lock primitives
- Heartbeats leases on a timer (rather than relying on Claude Code hooks)
- Multiplexes streaming output from all agents over a unix socket
- Syncs agent metadata to `refs/lazyspec/agents/{session-id}` for remote visibility
- Detects and recovers from agent crashes via process monitoring and lease admin-release

The TUI agents view is overhauled to connect to the daemon socket, rendering live streaming output in a scrollable panel rather than showing a static table of completed records.

This RFC supersedes RFC-016. Stories STORY-051, STORY-052, and STORY-053 should be deprecated and replaced by the stories identified here.

## Design

### Daemon Process

`lazyspec daemon start` forks a background process:

- PID file at `.lazyspec/daemon.pid`
- Unix socket at `.lazyspec/daemon.sock`
- Log output to `.lazyspec/daemon.log` (rotated by size)
- SIGTERM triggers graceful shutdown: stop accepting new work, wait for running agents to finish their current tool call, then exit
- SIGINT triggers immediate shutdown: send SIGKILL to child processes and exit

@ref src/tui/agent.rs#AgentSpawner

The existing `AgentSpawner` struct is replaced by daemon-side agent management. The TUI becomes a thin client that reads from the socket.

```
lazyspec daemon start          # fork to background
lazyspec daemon start --watch  # fork + auto-assign unclaimed iterations
lazyspec daemon stop           # SIGTERM, wait for graceful shutdown
lazyspec daemon status         # report running/stopped, agent count, uptime
```

### Spawning Claude Processes

The daemon spawns headless Claude via `std::process::Command`:

```bash
claude -p "<prompt>" \
  --output-format stream-json \
  --verbose \
  --include-partial-messages \
  --allowedTools "Read,Edit,Bash,Glob,Grep" \
  --append-system-prompt-file <context-file>
```

Each process gets environment variables:
- `LAZYSPEC_AGENT_ID` -- unique identifier for the agent (follows RFC-035's identity chain)
- `ASSIGNED_TASK` -- the iteration or document ID

The context file is generated per-agent by running `lazyspec context <iteration-id>` and writing the result to a temp file. This gives the agent the full document chain (RFC -> Story -> Iteration) as system prompt context.

The daemon reads each process's stdout line-by-line, parsing stream-json events. Each event is tagged with the agent's session ID and forwarded to subscribed IPC clients.

@ref src/tui/agent.rs#AgentRecord

### Work Assignment

Two modes of assignment:

_Explicit assignment_ via CLI:

```
lazyspec assign ITERATION-042
```

This sends an assign message to the daemon over the socket. The daemon claims the iteration (RFC-035 `claim`), generates the context, spawns Claude, and begins streaming.

_Watch mode_ via daemon flag:

```
lazyspec daemon start --watch
```

The daemon polls for unclaimed iterations in `todo` or `in-progress` status on a configurable interval. When it finds one and has capacity below `max_concurrent_agents`, it auto-assigns. This enables a "push iterations, walk away" workflow.

### Agent Metadata in Refs

Per-agent session metadata is stored as a commit on `refs/lazyspec/agents/{session-id}`:

@draft AgentMetadata {
    agent_id: String,
    session_id: String,
    assigned_task: String,
    status: AgentStatus,       // running, complete, failed, cancelled
    started_at: DateTime,
    last_heartbeat: DateTime,
    tokens_in: u64,
    tokens_out: u64,
    tools_used: u32,
    error: Option<String>,
}

The daemon updates the local ref on each significant event (tool call complete, result message, error, status change). Refs are pushed to the remote on a configurable interval (default 30s), batching updates to avoid excessive pushes.

Other clones can see agent progress by fetching `refs/lazyspec/agents/*`. The TUI falls back to reading these refs directly when the daemon isn't running, providing a read-only historical view.

### IPC Protocol

Unix socket at `.lazyspec/daemon.sock`. Newline-delimited JSON, one message per line.

Client to daemon:

| Message | Fields | Effect |
|---------|--------|--------|
| `subscribe` | `agent_id: "*"` or specific ID | Start receiving events for matching agents |
| `unsubscribe` | `agent_id` | Stop receiving events |
| `assign` | `task: "ITERATION-042"` | Claim and spawn an agent for the task |
| `cancel` | `agent_id` | Send SIGTERM to the agent process, release lease |
| `status` | -- | Request daemon status summary |

Daemon to client:

| Message | Fields | Description |
|---------|--------|-------------|
| `agent_event` | `agent_id`, stream-json fields | Forwarded stream-json event from a Claude process |
| `agent_status` | `agent_id`, `status`, metadata | Status change notification |
| `daemon_status` | `agents: [...]`, `uptime`, `watching` | Response to status request |
| `error` | `message`, `agent_id?` | Error notification |

The protocol is deliberately simple. No request IDs or correlation -- clients subscribe to a stream and react to events. The daemon is the single source of truth for agent state.

### TUI Agents View

The current agents view (`draw_agents_screen` in `src/tui/views/panels.rs`) renders a static table of `AgentRecord` structs loaded from JSON files. This is replaced entirely.

The new agents view has two panels:

_Left panel_ -- agent list. Each row shows: status icon (spinner/checkmark/cross), agent ID (truncated), assigned task, elapsed time. Sorted by start time, running agents first.

_Right panel_ -- live output for the selected agent. The daemon streams `text_delta` events from Claude's stream-json output. The TUI accumulates these into a scrollable buffer and renders them as they arrive. Tool calls are shown as collapsed summaries (tool name + result status) with expand-on-enter.

Status bar shows: daemon connection state, agent counts (active/complete/failed), total tokens consumed.

Keybindings:

| Key | Action |
|-----|--------|
| `a` | Assign new work (opens iteration picker) |
| `c` | Cancel selected agent |
| `r` | Restart failed agent (re-assign same task) |
| `Enter` | Open full output log in `$EDITOR` |
| `j/k` | Navigate agent list |
| `Tab` | Switch focus between panels |

When the daemon is not running, the view renders agent metadata from `refs/lazyspec/agents/*` as a read-only table. A banner indicates the daemon is offline and offers `d` to start it.

@ref src/tui/views/panels.rs#draw_agents_screen
@ref src/tui/state/forms.rs#AgentDialog

### Integration with RFC-035

The daemon is the orchestrator that RFC-035 anticipated in its hooks section. The relationship:

- The daemon calls `lazyspec claim <task> --agent-id <id>` before spawning each agent
- The daemon heartbeats leases on a timer (default every 5 minutes), not via Claude Code hooks. This is more reliable because the daemon controls the timer rather than depending on hook execution frequency.
- When an agent process exits non-zero or disappears, the daemon admin-releases the lease and updates the agent metadata ref to `failed`
- Agent identity: the daemon generates IDs and passes them via `$LAZYSPEC_AGENT_ID`. No ambiguity about who holds the lock.

The Claude Code hooks defined in RFC-035 (`session-start`, `post-tool-use`, `session-end`) are still useful for agents running _outside_ the daemon (e.g. a human running `claude -p` directly). The daemon and hooks are complementary, not competing.

### Configuration

```toml
[orchestration]
max_concurrent_agents = 4
heartbeat_interval = "5m"
metadata_push_interval = "30s"
agent_timeout = "2h"
claude_binary = "claude"
default_allowed_tools = "Read,Edit,Bash,Glob,Grep"
watch_poll_interval = "30s"
```

@ref .lazyspec.toml

### Graceful Degradation

| Scenario | Behaviour |
|----------|-----------|
| Remote unreachable | Daemon continues locally. Metadata push retries on next interval. Claims fail if they require remote (configurable). |
| Daemon not running | TUI shows read-only agent history from refs. `lazyspec assign` auto-starts the daemon. |
| Claude binary missing | Daemon logs error on assign attempt, does not crash. |
| Agent process crashes | Daemon detects via waitpid, admin-releases lease, marks agent as failed, pushes metadata. |
| Socket connection lost | TUI reconnects on next poll cycle. Missed events are recoverable from agent metadata refs. |

## Stories

1. Daemon lifecycle -- background process management (fork, PID file, socket bind, signal handling, logging). `lazyspec daemon start/stop/status` subcommands. No agent spawning yet.

2. Agent spawning and supervision -- daemon spawns Claude processes with stream-json, reads stdout, manages child process lifecycle. Process monitoring, crash detection, exit status handling. Context generation via `lazyspec context`.

3. Work assignment -- `lazyspec assign` CLI command that talks to daemon over socket. Claim integration with RFC-035 locks. Heartbeat timer for held leases.

4. Agent metadata refs -- `refs/lazyspec/agents/{session-id}` commit chain. Metadata struct, ref creation/update, periodic push to remote. `lazyspec agents` CLI to list status.

5. IPC protocol -- socket message format, subscribe/unsubscribe, event multiplexing from agent processes to connected clients. Error handling and reconnection.

6. TUI agents view overhaul -- two-panel layout, live streaming output rendering, agent list with status indicators, keybindings for assign/cancel/restart. Daemon connection management and offline fallback.

7. Watch mode -- `--watch` flag for auto-assignment. Poll for unclaimed iterations, concurrency control, configurable intervals.
