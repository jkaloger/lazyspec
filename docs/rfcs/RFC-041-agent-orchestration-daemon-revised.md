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
- `Store::load(&root, &config)` unifies all three backends behind a single read API.

RFC-036's design treats orchestration as a separate axis from these primitives. The right shape is the inverse: the daemon is a thin scheduler over existing lazyspec storage, lease, and document-type facilities. Workflow policy belongs in the repository as a versioned artifact, not in daemon code.

The orchestration vision requires:

- Multi-agent concurrency without working-tree contention
- Review cycles at multiple lifecycle points
- Status-driven re-engagement (e.g. PR review feedback)
- Manual kickoff and observability surfaces in the TUI
- Named agents per workflow phase, addable without refactoring

This RFC supersedes RFC-036 with a design that meets those requirements by composing existing primitives.

## Intent

Add a foreground-blocking orchestrator process that:

- Polls lazyspec documents (configurable type, default `story`) for agent-eligible work
- Acquires RFC-035 leases as the authoritative claim
- Creates an isolated git worktree per claim and runs lifecycle hooks
- Spawns Claude in headless mode, streaming events to subscribers
- Drives the agent through multiple turns with continuation logic until a handoff state is reached
- Releases the lease and exits cleanly on handoff or terminal status
- Exposes a unix socket IPC surface for TUI and CLI clients
- Persists per-agent metadata in `refs/lazyspec/agents/{session-id}` for cross-machine visibility

`lazyspec daemon` runs in the foreground and blocks. Backgrounding, log rotation, and supervision are the init system's job (systemd unit, launchd plist, or shell `&`/`nohup` for ad-hoc local use). The daemon does not fork, write a PID file, or self-supervise.

The daemon does not own claim state, work-source state, or workflow policy. Those live in lazyspec primitives and repository configuration. The daemon orchestrates them.

## Design

### Document layer (no new abstraction)

The daemon reads candidate documents through the existing `Store::load(&root, &config)` (production entry; `load_with_fs` is the test seam per dictum 4). The configured `claim_type` (e.g. `story`) selects which slice of the loaded store to scan. Backends (`filesystem`, `git-ref`, `github-issues`) all dispatch through the same load path; the daemon does not see the difference.

Eligibility filter:

- `status` is in `[orchestration] active_statuses` (default: `todo`, `in-progress`)
- `assignees` intersects `[orchestration] agent_users`
- No active RFC-035 lease on the document

This collapses the "which tracker" abstraction into the existing storage layer. There is no separate tracker trait.

### Eligibility metadata

Add `assignees: Vec<String>` to document frontmatter (initially on the configured `claim_type`, conceptually applicable to any type). RFC-037 maps `assignees` to GitHub issue assignees bidirectionally.

Validation lives at the I/O seam (dictum 4), not in the engine: the `github-issues` store impl rejects writes whose assignees do not resolve to real GitHub users (its existing concern); `filesystem` and `git-ref` stores accept free strings. The daemon's eligibility filter does not validate; it only matches against `agent_users`.

Bot accounts (e.g. `claude-bot`) appear in `[orchestration] agent_users`. The daemon dispatches only documents whose `assignees` overlap that set. Unassigned documents are not auto-eligible; assignment is explicit.

`lazyspec assign <DOC_ID> [--user <name>]` is the user-facing command for marking a document agent-eligible. It is a normal store write that adds an assignee to frontmatter. It does not require the daemon to be running. The daemon picks up the assignment on its next poll.

### Claim authority

RFC-035 leases are the source of truth for "is this document claimed". The daemon holds the lease (not the agent process). The daemon heartbeats on a timer (default 5 minutes).

`lease.agent` is set to `{host}:{session_id}` where `host` is a stable per-machine identifier (e.g. `gethostname()` plus a daemon-local UUID persisted in `.lazyspec/daemon-host-id`) and `session_id` is per-agent-session unique. This makes orphan recovery precise: leases prefixed with this host's id belong to this machine; sessions are individually addressable.

Tick-loop eligibility uses local ref reads only (no per-tick fetch). The daemon piggybacks a batched `git fetch refs/lazyspec/leases/*` on the metadata-push interval (default 30s) to keep the local view fresh. Acquire-time CAS against all-zeros SHA is the safety net: if a stale local view leads to a contended acquire, CAS fails and the candidate is skipped this tick.

On daemon boot, leases prefixed with this host's id are reconciled. The daemon owns the child stdout pipes; if the daemon died, its agents died with it (or were reparented to init with no IPC path back to a new daemon). Recovery is therefore not "re-attach" but "wait, then release":

- Wait one RFC-035 lease `grace_period` (reuse the existing primitive's default; do not introduce a daemon-specific timer).
- Admin-release the lease.
- Leave the worktree in place (cheap, allows forensics; tick loop will reuse the branch on re-dispatch).
- Mark `refs/lazyspec/agents/{session-id}` status as `crashed`.

The daemon's in-memory state is a dispatch cache: pid tracking, stream multiplexing, retry-queue timer handles, token totals. None of it is authoritative.

Restart recovery is lease-driven and process-driven. No daemon database.

### Workspace per claim

Each claim gets a git worktree. Default branch template:

```toml
[orchestration]
branch_template = "agents/{{ story_id }}"
workspace_root = ".lazyspec/work"
```

Template variables: `iteration_id`, `iteration_slug`, `agent_id`, `story_id`, `date`. Branch templates and prompt templates share one engine: **minijinja** (Jinja2 syntax, strict undefined, sandboxed; ~150kb, actively maintained). Branches don't need conditionals/loops in practice but use the same renderer for consistency. Post-render output is sanitized against `git check-ref-format --branch`.

The worktree branch is created from the configured base (default `origin/main`) on first claim. Re-engagement reuse rule: if the local branch ref exists, reuse the worktree. If the ref is gone (deleted post-merge, never created, or pruned), `git worktree add` creates it fresh from the base. WIP recovery is the agent's responsibility once it attaches to an existing worktree; the daemon does not reset or rewind branch state.

Worktree-only in v1. Disk cost is one repo clone per concurrent agent; `max_concurrent_agents` is the budget knob. No separate disk-quota config.

The existing string-substitution `render_template` in `src/engine/template.rs` is left in place for `resolve_filename`; it migrates to minijinja only when a second concrete need for richer templating in that path appears (dictum 6).

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

Prompt template lives in `.lazyspec/prompts/<role>.md` (markdown body). v1 ships a single role: `builder`. Rendered with **minijinja** in strict-undefined mode; unknown variables fail at config load.

Render variables:

- `doc`: full normalized document (id, title, body, status, assignees, context_chain)
- `attempt`: null on first turn, integer on continuation
- `prior_iterations`: list of iteration IDs created so far in this session

`prior_iterations` is computed by store-diff against a session-start snapshot: at session start the daemon records the set of iteration ids with `implements: <story_id>`; each turn the daemon re-queries the store and reports the delta. The source is the store, not the agent's tool-call stream, so it is decoupled from stream-json format and survives daemon restart (snapshot can be reconstructed from the agent metadata ref).

The prompt's job is to instruct the agent to perform the workflow phase: write iterations, implement, push, open PR, set status to a handoff state. The daemon does not enforce these steps; the prompt does. Agent tools are `lazyspec` (CLI), `git`, `gh`, `bash`, `edit`. The tool allow-list comes from `[orchestration.runtime] allowed_tools`.

Trajectory: when a second role lands, the layout becomes `.lazyspec/workflows/<role>.md` with per-role front matter (claim_type, hooks, handoff_states, allowed_tools) plus the prompt body. Single-role v1 keeps the simpler split.

### Orchestrator tick loop

Every `[orchestration] poll_interval_ms` (default 30s):

1. Reconcile running agents:
   - Stall detection: if no stream-json events arrive from a running agent for `stall_timeout_ms` (default 5 minutes), SIGTERM and queue retry. The stall timer is suspended while any `tool_use` is in-flight (between `tool_use` start and matching `tool_result`); a long-running `Bash(cargo test)` does not look like a stall. Stall is independent of lease heartbeat: the daemon timer keeps the lease alive regardless of agent activity. `turn_timeout_ms` (default 1h) remains the hard wall for any single turn.
   - Status refresh: re-load each running document's status. Terminal → kill agent, release lease, remove worktree. Handoff → kill, release lease, keep worktree. Active → continue.
2. Fetch candidates via `Store::load(&root, &config)` and slice by `claim_type`.
3. Filter by eligibility: `status ∈ active_statuses`, `assignees ∩ agent_users ≠ ∅`, no active local lease ref.
4. Sort: priority asc, created_at asc, id asc.
5. Dispatch while concurrency slots are available.

Preflight (workflow file readable, prompt renders, `agent_users` non-empty) runs at daemon start and is invalidated by `notify` events on the config and prompt files, not on every tick. Claude-binary preflight is dropped: subprocess spawn at dispatch time is the only honest test, and a missing binary surfaces there with a clear error.

### Retry semantics

Continuation (clean exit, status still active): 1-second delay, fresh `claude -p` invocation in the same workspace. `attempt` increments. Cap: `max_turns = 20`.

Failure (abnormal exit, stall, hook failure): exponential backoff `delay = min(10000 * 2^(n-1), max_retry_backoff_ms)`, default cap 5 minutes. Cap: `max_failure_attempts = 5`. After cap, the daemon releases the lease and emits a `failed` agent event; status mutation is the workflow's responsibility, not the daemon's.

### IPC and CLI surface

Unix socket at `.lazyspec/daemon.sock`. Newline-delimited JSON, one message per line. Unix-only in v1; Windows named-pipe support is deferred.

Client to daemon: `subscribe`, `unsubscribe`, `cancel`, `status`, `kick` (optional rescan-now nudge to skip the next poll wait after a frontmatter change).
Daemon to client: `agent_event`, `agent_status`, `daemon_status`, `error`.

There is no `assign` IPC message. Assignment is a frontmatter mutation through the normal store path; the daemon discovers it on the next poll.

CLI subcommands:

- `lazyspec daemon` — run the orchestrator in the foreground; blocks until SIGTERM/SIGINT.
- `lazyspec daemon status [--json]` — request snapshot over socket. Connects, reads, exits. Does not fork the daemon if absent; returns "daemon not running" cleanly.
- `lazyspec assign <DOC_ID> [--user <name>]` — store write that adds an assignee to the document's frontmatter. Default `--user` is the first entry in `agent_users`. Works without the daemon. If the daemon socket is reachable, follows up with a `kick` message to cut dispatch latency.

Backgrounding is the init system's job. Document a sample systemd unit and launchd plist in the user guide; for ad-hoc local use, `lazyspec daemon &` or `nohup lazyspec daemon` are sufficient. There is no `lazyspec daemon stop`: use `systemctl stop`, `launchctl unload`, or `kill` against whatever supervisor is running it.

All commands honor `--json` per dictum 2.

### TUI integration

The agents view consumes socket events for live streaming. Manual kickoff: a hotkey opens a document picker, performs the same store-write as `lazyspec assign` (frontmatter mutation), and sends a `kick` over the socket if the daemon is reachable to skip the poll wait. When the daemon is offline, the assignment still persists; the view falls back to reading `refs/lazyspec/agents/*` for a read-only history.

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
branch_template = "agents/{{ story_id }}"
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
| Daemon not running      | TUI reads `refs/lazyspec/agents/*` for read-only history. `lazyspec assign` succeeds (store write); dispatch happens whenever the daemon next runs. |
| Claude binary missing   | Subprocess spawn at dispatch time fails with a clear error; daemon logs and queues retry. Does not crash.       |
| Agent crashes           | Daemon detects via waitpid, queues retry with backoff, updates metadata ref.                                   |
| Hook timeout            | Treated as failure per the hook's failure semantics above.                                                     |
| Worktree creation fails | Single attempt is failed and retried; persistent failure releases the lease after `max_failure_attempts`.      |

## Stories

1. `assignees` frontmatter + `[orchestration] agent_users` + RFC-037 mapping + `lazyspec assign` — frontmatter schema extension on the configured `claim_type`, GitHub assignees bidirectional sync, per-backend validation in store impls, and the `lazyspec assign` CLI as a normal store write. Prereq for any tick-loop eligibility logic.

2. Daemon process lifecycle — `lazyspec daemon` foreground-blocking entry point, unix socket bind, SIGTERM/SIGINT graceful shutdown. No fork, no PID file. Sample systemd unit + launchd plist documented. No agent spawning yet.

3. `AgentRunner` trait, `ClaudeP` impl, worktree + hooks lifecycle — subprocess seam, stream-json parsing, normalized event emission. Worktree creation, branch-ref-exists reuse rule, branch templating via minijinja, four hooks with documented failure semantics.

4. Orchestrator tick loop — polling, eligibility filter (local lease ref read), RFC-035 lease acquire/heartbeat/release with `lease.agent = {host}:{session_id}`, batched lease fetch piggybacked on metadata-push interval, dispatch with concurrency control, retry queue (continuation + failure backoff), reconciliation (tool-call-aware stall + status refresh), boot orphan recovery (wait RFC-035 grace_period, admin-release, keep worktree).

5. Prompt rendering — minijinja in strict-undefined mode, `.lazyspec/prompts/builder.md`, render variables (`doc`, `attempt`, `prior_iterations` via session-start store-diff), notify-driven hot-reload + preflight invalidation.

6. IPC socket protocol + CLI surface — message format, `subscribe`/`unsubscribe`/`cancel`/`status`/`kick`, event multiplexing, `lazyspec daemon status --json`, error handling, reconnection.

7. TUI agents view — two-panel layout, live streaming, manual kickoff hotkey (frontmatter mutation + optional `kick`), daemon connection state, offline fallback to refs.

8. Agent metadata refs — `refs/lazyspec/agents/{session-id}` commit chain, periodic push, cross-machine read path, `crashed` state on boot recovery.
