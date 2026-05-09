---
title: Agent Orchestration Daemon (revised)
type: rfc
status: draft
author: jkaloger
date: 2026-05-09
tags: []
related:
  - supersedes: RFC-036
  - related-to: RFC-035
  - related-to: RFC-037
---

## Problem

RFC-036 sketched an agent orchestration daemon coupled to Claude Code hooks, JSON metadata files, and per-iteration claim CLI. Since RFC-036 was drafted, lazyspec gained primitives that change the design surface:

- RFC-035 lease engine: distributed claims via git refs with CAS, heartbeat, expiry, grace period. Distributed-safe across machines.
- RFC-035 git-ref document storage: iterations live in `refs/lazyspec/iteration/*`, invisible to the working tree but visible to lazyspec.
- RFC-037 GitHub Issues store: `assignees`/`tags`/`status` already round-trip through the issue model.
- `Store::load_with_fs` unifies all three backends behind a single read API.

RFC-036's design treats orchestration as a separate axis from these primitives. The right shape is the inverse: the daemon is a thin scheduler over existing lazyspec storage, lease, and document-type facilities. Workflow policy belongs in the repository as a versioned artifact, not in daemon code.

The orchestration vision requires:

- Multi-agent concurrency without working-tree contention
- Review cycles at multiple lifecycle points
- Status-driven re-engagement (e.g. PR review feedback)
- Manual kickoff and observability surfaces in the TUI
- Named agents per workflow phase, addable without refactoring

This RFC supersedes RFC-036 with a design that meets those requirements by composing existing primitives.

## Intent

Add a long-running orchestrator daemon that:

- Polls lazyspec documents (configurable type, default `story`) for agent-eligible work
- Acquires RFC-035 leases as the authoritative claim
- Creates an isolated git worktree per claim and runs lifecycle hooks
- Spawns Claude in headless mode, streaming events to subscribers
- Drives the agent through multiple turns with continuation logic until a handoff state is reached
- Releases the lease and exits cleanly on handoff or terminal status
- Exposes a unix socket IPC surface for TUI and CLI clients
- Persists per-agent metadata in `refs/lazyspec/agents/{session-id}` for cross-machine visibility

The daemon does not own claim state, work-source state, or workflow policy. Those live in lazyspec primitives and repository configuration. The daemon orchestrates them.

## Design

### Document layer (no new abstraction)

The daemon reads candidate documents through the existing `Store::load_with_fs`. The configured `claim_type` (e.g. `story`) selects which type to poll. Backends (`filesystem`, `git-ref`, `github-issues`) all dispatch through the same load path; the daemon does not see the difference.

Eligibility filter:

- `status` is in `[orchestration] active_statuses` (default: `todo`, `in-progress`)
- `assignees` intersects `[orchestration] agent_users`
- No active RFC-035 lease on the document

This collapses the "which tracker" abstraction into the existing storage layer. There is no separate tracker trait.

### Eligibility metadata

Add `assignees: Vec<String>` to document frontmatter (initially on the configured `claim_type`, conceptually applicable to any type). RFC-037 maps `assignees` to GitHub issue assignees bidirectionally; values must resolve to real GitHub users for `github-issues` documents and are free strings for `filesystem` and `git-ref` documents.

Bot accounts (e.g. `claude-bot`) appear in `[orchestration] agent_users`. The daemon dispatches only documents whose `assignees` overlap that set. Unassigned documents are not auto-eligible; assignment is explicit.

### Claim authority

RFC-035 leases are the source of truth for "is this document claimed". The daemon holds the lease (not the agent process). The daemon heartbeats on a timer (default 5 minutes). On daemon boot, leases held by any of the configured `agent_users` identities are reconciled against running processes:

- Process running locally → re-attach
- Process gone → admin-release the lease after the grace period

The daemon's in-memory state is a dispatch cache: pid tracking, stream multiplexing, retry-queue timer handles, token totals. None of it is authoritative.

Restart recovery is lease-driven and process-driven. No daemon database.

### Workspace per claim

Each claim gets a git worktree. Default branch template:

```toml
[orchestration]
branch_template = "agents/{story_id}"
workspace_root = ".lazyspec/work"
```

Template variables: `{iteration_id}`, `{iteration_slug}`, `{agent_id}`, `{story_id}`, `{date}`. The same Liquid renderer used for prompts (one engine, two consumers) renders branch names. Post-render output is sanitized against `git check-ref-format --branch`.

The worktree branch is created from the configured base (default `origin/main`) on first claim. Subsequent re-engagements (e.g. after PR review feedback) reuse the worktree if it still exists.

For non-git workflows, `workspace.kind = "directory"` opts out of worktree creation; the daemon `mkdir -p`s the path and lets `after_create` populate it.

### Hooks

Four lifecycle points. All execute via `bash -lc <script>` with cwd set to the workspace path and a configurable timeout (default 60s).

| Hook            | When                          | Failure                  |
| --------------- | ----------------------------- | ------------------------ |
| `after_create`  | Workspace first created       | Fatal to creation        |
| `before_run`    | Each turn, before agent spawn | Fatal to current attempt |
| `after_run`     | Each turn, after agent exit   | Logged, ignored          |
| `before_remove` | Workspace teardown            | Logged, ignored          |

Environment exposed to all hooks:

- `LAZYSPEC_DOC_ID`, `LAZYSPEC_DOC_TYPE`
- `LAZYSPEC_AGENT_ID`, `LAZYSPEC_BRANCH`, `LAZYSPEC_WORKSPACE`

### Agent runtime protocol

`AgentRunner` trait at the subprocess seam (dictum 4). v1 ships a single concrete impl that invokes `claude -p --output-format stream-json` and consumes the stream.

@draft AgentRunner {
fn spawn(&self, ctx: AgentContext) -> Result<AgentHandle>;
}

@draft AgentContext {
workspace: PathBuf,
prompt: String,
agent_id: String,
session_id: String,
env: HashMap<String, String>,
}

@draft AgentHandle {
pid: u32,
events: Receiver<AgentEvent>,
cancel: oneshot::Sender<()>,
}

@draft AgentEvent {
SessionStarted { session_id: String, pid: u32 },
Text { delta: String },
ToolCall { name: String, summary: String, status: ToolStatus },
TurnCompleted { tokens_in: u64, tokens_out: u64 },
TurnFailed { error: String },
SubprocessExited { code: i32 },
}

The trait is sealed over normalized events. v1 ClaudeP impl parses stream-json and emits these events. Future bidirectional impls (Agent SDK sidecar, direct API client) implement the same trait.

State is carried between turns via the worktree (committed code), lazyspec documents (created iterations, status), and the prompt template's continuation variables. There is no in-process thread that survives across turns in v1.

### Prompt rendering

Prompt template lives in `.lazyspec/prompts/<role>.md` (markdown body). v1 ships a single role: `builder`. Rendered with strict Liquid; unknown variables fail at config load.

Render variables:

- `doc`: full normalized document (id, title, body, status, assignees, context_chain)
- `attempt`: null on first turn, integer on continuation
- `prior_iterations`: list of iteration IDs created so far in this session

The prompt's job is to instruct the agent to perform the workflow phase: write iterations, implement, push, open PR, set status to a handoff state. The daemon does not enforce these steps; the prompt does. Agent tools are `lazyspec` (CLI), `git`, `gh`, `bash`, `edit`. The tool allow-list comes from `[orchestration.runtime] allowed_tools`.

Trajectory: when a second role lands, the layout becomes `.lazyspec/workflows/<role>.md` with per-role front matter (claim_type, hooks, handoff_states, allowed_tools) plus the prompt body. Single-role v1 keeps the simpler split.

### Orchestrator tick loop

Every `[orchestration] poll_interval_ms` (default 30s):

1. Reconcile running agents:
   - Stall detection: if no events from a running agent for `stall_timeout_ms` (default 5 minutes), SIGTERM and queue retry.
   - Status refresh: re-load each running document's status. Terminal → kill agent, release lease, remove worktree. Handoff → kill, release lease, keep worktree. Active → continue.
2. Preflight validation: workflow file readable, `agent_users` non-empty, prompt template renders, claude binary discoverable. Failure skips dispatch but does not abort reconciliation.
3. Fetch candidates via `Store::load(claim_type)`.
4. Filter by eligibility (status, assignees, no active lease).
5. Sort: priority asc, created_at asc, id asc.
6. Dispatch while concurrency slots are available.

### Retry semantics

Continuation (clean exit, status still active): 1-second delay, fresh `claude -p` invocation in the same workspace. `attempt` increments. Cap: `max_turns = 20`.

Failure (abnormal exit, stall, hook failure): exponential backoff `delay = min(10000 * 2^(n-1), max_retry_backoff_ms)`, default cap 5 minutes. Cap: `max_failure_attempts = 5`. After cap, the daemon releases the lease and emits a `failed` agent event; status mutation is the workflow's responsibility, not the daemon's.

### IPC and CLI surface

Unix socket at `.lazyspec/daemon.sock`. Newline-delimited JSON, one message per line.

Client to daemon: `subscribe`, `unsubscribe`, `assign`, `cancel`, `status`.
Daemon to client: `agent_event`, `agent_status`, `daemon_status`, `error`.

CLI subcommands:

- `lazyspec daemon start [--watch]` — fork to background; `--watch` enables polling
- `lazyspec daemon stop` — SIGTERM, await graceful shutdown
- `lazyspec daemon status [--json]` — request snapshot over socket
- `lazyspec assign <DOC_ID>` — manual claim; spawns daemon if not running

All commands honor `--json` per dictum 2.

### TUI integration

The agents view consumes socket events for live streaming. Manual kickoff: a hotkey opens a document picker, sends `assign` to the daemon. When the daemon is offline, the view falls back to reading `refs/lazyspec/agents/*` for a read-only history.

The view is two-panel: agent list on the left (status icon, doc id, elapsed), live output on the right. Status bar: daemon connection, agent counts, total tokens.

### Agent metadata refs

`refs/lazyspec/agents/{session-id}` stores per-session metadata as a commit chain (same pattern as RFC-035 leases). Pushed to the remote on a configurable interval (default 30s). Other clones fetch and read for cross-machine visibility.

@draft AgentMetadata {
agent_id: String,
session_id: String,
doc_id: String,
doc_type: String,
status: AgentStatus,
started_at: DateTime<Utc>,
last_event_at: DateTime<Utc>,
tokens_in: u64,
tokens_out: u64,
turn_count: u32,
error: Option<String>,
}

### Configuration

```toml
[orchestration]
agent_users = ["claude-bot"]
claim_type = "story"
poll_interval_ms = 30000
max_concurrent_agents = 4
active_statuses = ["todo", "in-progress"]
handoff_states = ["in-review"]
branch_template = "agents/{story_id}"
workspace_root = ".lazyspec/work"
prompt_path = ".lazyspec/prompts/builder.md"
max_turns = 20
max_failure_attempts = 5
max_retry_backoff_ms = 300000
stall_timeout_ms = 300000

[orchestration.hooks]
after_create = ""
before_run = "cargo build && cargo test"
after_run = ""
before_remove = ""
timeout_ms = 60000

[orchestration.runtime]
claude_binary = "claude"
allowed_tools = "Read,Edit,Bash,Glob,Grep"
turn_timeout_ms = 3600000
```

The `[orchestration]` section is hot-reloaded on file change; new values apply to future ticks. In-flight agent sessions are not restarted on config change.

### Graceful degradation

| Scenario                | Behaviour                                                                                                      |
| ----------------------- | -------------------------------------------------------------------------------------------------------------- |
| Remote unreachable      | Lease operations fail (RFC-035 baseline). Daemon continues local-only. Metadata push retries on next interval. |
| Daemon not running      | TUI reads `refs/lazyspec/agents/*` for read-only history. `lazyspec assign` auto-starts the daemon.            |
| Claude binary missing   | Daemon logs error on assign attempt, does not crash.                                                           |
| Agent crashes           | Daemon detects via waitpid, queues retry with backoff, updates metadata ref.                                   |
| Hook timeout            | Treated as failure per the hook's failure semantics above.                                                     |
| Worktree creation fails | Single attempt is failed and retried; persistent failure releases the lease after `max_failure_attempts`.      |

## Stories

1. Daemon process lifecycle — `lazyspec daemon start/stop/status` subcommands. Fork to background, PID file, unix socket bind, signal handling, log rotation. No agent spawning yet.

2. `AgentRunner` trait, `ClaudeP` impl, worktree + hooks lifecycle — subprocess seam, stream-json parsing, normalized event emission. Worktree creation/reuse, branch templating, four hooks with documented failure semantics.

3. Orchestrator tick loop — polling, eligibility filter, RFC-035 lease acquire/heartbeat/release, dispatch with concurrency control, retry queue (continuation + failure backoff), reconciliation (stall + status refresh).

4. Prompt rendering — Liquid engine, `.lazyspec/prompts/builder.md`, render variables (`doc`, `attempt`, `prior_iterations`), strict unknown-variable failure, hot-reload.

5. IPC socket protocol + CLI surface — message format, subscribe/unsubscribe, event multiplexing, `lazyspec assign`, `lazyspec daemon status --json`, error handling, reconnection.

6. TUI agents view — two-panel layout, live streaming, manual kickoff hotkey, daemon connection state, offline fallback to refs.

7. Agent metadata refs — `refs/lazyspec/agents/{session-id}` commit chain, periodic push, cross-machine read path.

8. `assignees` frontmatter + `[orchestration] agent_users` + RFC-037 mapping — frontmatter schema extension, GitHub assignees bidirectional sync, validation rules.
