<h1 align="center">
  🤖
  <br>lazyspec
</h1>
<p align="center">
    A little TUI & CLI for project documentation.
</p>

<p align="center">
  <a href="https://github.com/jkaloger/lazyspec/actions/workflows/ci.yml"><img src="https://github.com/jkaloger/lazyspec/actions/workflows/ci.yml/badge.svg?branch=main" alt="CI"></a>
  <img src="https://img.shields.io/badge/rust-2021-orange?logo=rust&logoColor=white" alt="Rust 2021">
  <img src="https://img.shields.io/badge/status-experimental-blueviolet" alt="Status: Experimental">
  <img src="https://img.shields.io/github/v/tag/jkaloger/lazyspec?label=version&color=blue" alt="Version">
  <a href="https://github.com/jkaloger/lazyspec/commits/main"><img src="https://img.shields.io/github/last-commit/jkaloger/lazyspec?logo=git&logoColor=white" alt="Last commit"></a>
  <a href="https://github.com/jkaloger/lazyspec/blob/main/flake.nix"><img src="https://img.shields.io/badge/nix-flake-5277C3?logo=nixos&logoColor=white" alt="Nix Flake"></a>
</p>

<img width="1864" height="1147" alt="screenshot of a terminal interface displaying codebase documentation, categorised by type" src="https://github.com/user-attachments/assets/91f308d1-8d03-4815-b2ec-fa445159c563" />

> [!WARNING]
> Lazyspec is experimental. APIs and CLI interfaces will change frequently and without notice.

## Features

Lazyspec manages project documentation as version-controlled markdown files with YAML frontmatter. Documents live in your repo, so agents and humans read from the same source of truth.

- Create, update, link, and validate documents. Typed relationships (`implements`, `supersedes`, `blocks`, `related-to`) keep the chain explicit.
- Catch broken links, orphaned documents, and incomplete frontmatter before they rot. `lazyspec validate` exits non-zero on errors, so it slots into CI.
- Embed `@ref` directives in your specs to point at source code. Lazyspec expands them inline using `git show`, with symbol-level extraction for Rust and TypeScript.
- Fuzzy search, markdown preview, live file watching, and document creation without leaving the terminal.
- Every command supports `--json` output for automation and agent integration.
- Define your own types, templates, and directory layout in `.lazyspec.toml`.

## Install

### Nix

```sh
nix profile install github:jkaloger/lazyspec
```

Or run without installing:

```sh
nix run github:jkaloger/lazyspec
```

### Cargo

```sh
cargo install --git https://github.com/jkaloger/lazyspec
```

### From Source

```sh
git clone https://github.com/jkaloger/lazyspec
cd lazyspec
cargo install --path .
```

### Shell Completions

Generate and source a completion script for your shell:

```sh
# zsh
source <(lazyspec completions zsh)

# bash
source <(lazyspec completions bash)

# fish
lazyspec completions fish | source
```

Add the appropriate line to your shell profile (`~/.zshrc`, `~/.bashrc`, etc.) to load completions on startup. Completions include subcommands, flags, document IDs, and relationship types.

## Skills

Lazyspec includes a set of agent skills that enforce its workflow:

| Skill              | Purpose                                                                |
| ------------------ | ---------------------------------------------------------------------- |
| `plan-work`        | Detect existing artifacts and determine the right entry point          |
| `write-rfc`        | Propose a design with intent, interface sketches, and identify stories |
| `create-story`     | Create stories with acceptance criteria linked to an RFC               |
| `resolve-context`  | Gather full document chain (RFC -> Story -> Iteration) before work     |
| `create-iteration` | Plan an iteration with task breakdown and test plan                    |
| `build`            | Implement tasks from an iteration with subagent dispatch               |
| `review-iteration` | Two-stage review -- AC compliance first, then code quality             |
| `create-audit`     | Criteria-based review (health check, security, accessibility, etc.)    |

## Usage

### Quick Start

Initialise a new project, then launch the TUI:

```sh
lazyspec init
lazyspec
```

> [!TIP]
> Check the `examples/` directory for a complete project setup including config, templates, and agent skill definitions you can use as a starting point.
> This repo dogfoods lazyspec, so you can also check out the `docs/` directory or run `lazyspec` from this repo.

### TUI

Running `lazyspec` with no subcommand opens the interactive dashboard. It provides fuzzy search, markdown preview, document creation, and live file watching -- documents update automatically when changed on disk.

<details>
<summary><h3>CLI</h3></summary>

All document management is available as subcommands. Most accept `--json` for machine-readable output.

| Command                              | Description                                                           |
| ------------------------------------ | --------------------------------------------------------------------- |
| `init`                               | Initialise lazyspec in the current project                            |
| `create <type> <title> [--author X]` | Create a document (rfc, adr, story, iteration)                        |
| `list [type] [--status X]`           | List documents with optional filters                                  |
| `show <id> [-e]`                     | Display a document by path or shorthand ID (e.g. `RFC-001`)           |
| `update <path> --status X --title X` | Update document frontmatter                                           |
| `delete <path>`                      | Delete a document                                                     |
| `link <from> <rel> <to>`             | Add a typed relationship (implements, supersedes, blocks, related-to) |
| `unlink <from> <rel> <to>`           | Remove a relationship between documents                               |
| `assign <id> [--user X]`             | Append a user to a document's `assignees` frontmatter list            |
| `search <query> [--doc-type X]`      | Full-text search across all documents                                 |
| `context <id>`                       | Show the full document chain (RFC -> Story -> Iteration)              |
| `status`                             | Show full project status with all documents and validation            |
| `ignore <path>`                      | Mark a document to skip validation                                    |
| `unignore <path>`                    | Remove validation skip from a document                                |
| `validate [--warnings]`              | Check document integrity and link consistency                         |
| `fix [paths] [--dry-run]`            | Fix documents with broken or incomplete frontmatter                   |
| `pin <id>`                           | Pin blob hashes onto `@ref` directives in a document                  |
| `provenance add <id> <citation>`     | Append a citation to a document's provenance list                     |
| `provenance remove <id> <citation>`  | Remove an exact-match citation from a document's provenance list      |
| `provenance list [id]`               | List citations for a document, or for all documents grouped by id     |
| `reservations list`                  | Show all reservation refs on the remote                               |
| `reservations prune [--dry-run]`     | Remove refs for documents that already exist locally                  |
| `daemon`                             | Run the orchestration daemon in the foreground                        |

#### `show` Flags

| Flag                        | Description                                      |
| --------------------------- | ------------------------------------------------ |
| `-e`, `--expand-references` | Expand `@ref` directives into fenced code blocks |
| `--max-ref-lines N`         | Max lines per expanded ref (default: 25)         |

#### `provenance` Subcommands

Cite the sources of truth that informed a document. Citations are free-form strings stored as a YAML list in frontmatter.

```sh
lazyspec provenance add RFC-001 "Workshop 2026-04-12"
lazyspec provenance add RFC-001 "Privacy Act 1988"
lazyspec provenance list RFC-001
# Workshop 2026-04-12
# Privacy Act 1988

lazyspec provenance remove RFC-001 "Privacy Act 1988"
lazyspec provenance list
# RFC-001	Workshop 2026-04-12
```

All three subcommands accept `--json`. Shapes:

- `add` / `remove`: `{ "doc": "...", "added"|"removed": "...", "provenance": [...] }`
- `list <id>`: `{ "doc": "...", "provenance": [...] }`
- `list` (no id): `{ "documents": [{ "id": "...", "path": "...", "provenance": [...] }, ...] }`

`add` rejects empty citations. `remove` is exact-match and errors when the citation is absent.

#### `assign`

Append a user login to a document's `assignees` frontmatter list. When `--user` is omitted, the first entry of `[orchestration] agent_users` is used; with neither flag nor default the command errors.

```sh
lazyspec assign STORY-126 --user claude-bot
# Assigned claude-bot to STORY-126 (assignees: ["claude-bot"])

lazyspec assign STORY-126 --json
# {"id":"STORY-126","assignee_added":"claude-bot","assignees":["claude-bot"]}
```

## Coordination

### Claude Code Hooks

Lazyspec ships hook snippets that claim, heartbeat, and release a lease on `$ASSIGNED_TASK` across a Claude Code session. The orchestrator (daemon, manual `export`, etc.) sets the env var; hooks no-op silently when it is unset, so the snippet is safe to install unconditionally.

Drop into `.claude/settings.json`:

```json
{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "[ -n \"$ASSIGNED_TASK\" ] && lazyspec claim \"$ASSIGNED_TASK\" --agent-id \"$CLAUDE_SESSION_ID\" --json || true"
          }
        ]
      }
    ],
    "PostToolUse": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "[ -n \"$ASSIGNED_TASK\" ] && lazyspec heartbeat \"$ASSIGNED_TASK\" --agent-id \"$CLAUDE_SESSION_ID\" --min-interval 15m --json || true"
          }
        ]
      }
    ],
    "SessionEnd": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "[ -n \"$ASSIGNED_TASK\" ] && lazyspec release \"$ASSIGNED_TASK\" --agent-id \"$CLAUDE_SESSION_ID\" --json || true"
          }
        ]
      }
    ]
  }
}
```

The standalone file lives at [`hooks/claude-code-settings.json`](hooks/claude-code-settings.json).

**`$ASSIGNED_TASK` contract.** Orchestrator sets it to a doc id (e.g. `ITERATION-170`). If unset, the `[ -n "$ASSIGNED_TASK" ]` guard short-circuits and no `lazyspec` invocation happens.

**Throttle.** `--min-interval 15m` matches the default `lease_duration / 4` (lease defaults to 60m). If you tune `lease_duration` in `.lazyspec.toml`, tune this to roughly a quarter of it.

**Error tolerance.** `|| true` swallows non-zero exits from `lazyspec` (e.g. lease already released, network blip), so a session never fails to end because of a coordination error.

See [RFC-035](docs/rfcs/RFC-035-git-ref-document-storage-with-lease-based-claiming.md) for the design rationale.

</details>

## Daemon Deployment

The orchestration daemon runs in the foreground and binds a unix socket at `.lazyspec/daemon.sock`. For production use, supervise it with a process manager -- systemd on Linux, launchd on macOS -- rather than backgrounding it manually.

### systemd

```ini
[Unit]
Description=lazyspec orchestration daemon
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/lazyspec daemon
WorkingDirectory=/srv/lazyspec/your-repo
Restart=on-failure
User=lazyspec

[Install]
WantedBy=multi-user.target
```

Replace `WorkingDirectory` with the absolute path to your repo. Install at `/etc/systemd/system/lazyspec.service`, then enable and start it with `systemctl enable --now lazyspec`.

### launchd

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>au.com.inlight.lazyspec</string>
    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/lazyspec</string>
        <string>daemon</string>
    </array>
    <key>WorkingDirectory</key>
    <string>/Users/you/your-repo</string>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>/tmp/lazyspec.out.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/lazyspec.err.log</string>
</dict>
</plist>
```

Install at `~/Library/LaunchAgents/au.com.inlight.lazyspec.plist` and load with `launchctl load ~/Library/LaunchAgents/au.com.inlight.lazyspec.plist`.

### Stopping

There is no `lazyspec daemon stop` subcommand. Stop the daemon through its supervisor: `systemctl stop lazyspec`, `launchctl unload ~/Library/LaunchAgents/au.com.inlight.lazyspec.plist`, or `kill <pid>`. Both SIGTERM and SIGINT trigger graceful shutdown.

### Startup sequence

`lazyspec daemon` runs these steps in order on every start:

1. **Bind socket.** Single-instance gate. If another daemon already listens, exit.
2. **Boot orphan recovery.** Scan local lease refs for leases owned by this host (agent prefix `{host_id}:`). If any are found, the daemon **blocks for `coordination.grace_period`** to give a still-running peer process time to renew; then admin-releases each orphan lease and marks the corresponding `refs/lazyspec/agents/{session_id}` as `crashed`. Worktrees are left in place so an operator can resume or discard them. With the RFC-035 default `grace_period = "1h"`, a crash recovery can stall startup for an hour -- tune `grace_period` in `.lazyspec.toml` if that's not acceptable.
3. **Preflight checks.** Three gates: (a) the prompt template `.lazyspec/prompts/builder.md` is readable, (b) the prompt template renders against a dummy context under minijinja strict-undefined mode, (c) `orchestration.agent_users` is non-empty. Failure does **not** stop the daemon; it logs the failed checks and gates new dispatches. In-flight agents are not affected.
4. **Accept thread + tick thread.** The tick loop is started with the initial preflight report and a notify-driven watcher on `.lazyspec.toml` + `.lazyspec/prompts/builder.md`.

Edit either watched file at runtime and the daemon re-runs preflight before the next dispatch. If preflight now fails, the daemon stops issuing new dispatches but does not yank in-flight agents -- hot-reload applies to future ticks only.

<details>
<summary><h3><code>@ref</code> Syntax</h3></summary>

Documents can embed references to source code using `@ref` directives. By default, `lazyspec show` renders them as-is. Pass `-e` to expand them inline.

```
@ref <path>                    # entire file
@ref <path>#<symbol>           # specific type or struct
@ref <path>#<symbol>@<sha>     # symbol at a specific git commit
@ref <path>#123                # line 123
@ref <path>#123@<sha>          # line 123 at a specific git commit
```

Expansion resolves content via `git show` (committed state, not working tree). Supported languages for symbol extraction are TypeScript (`.ts`/`.tsx`) and Rust (`.rs`).

Each expanded ref includes a caption line showing the file path, short git SHA, and symbol or line info. Expanded blocks are truncated to 25 lines by default; when truncated, a trailing comment shows how many lines were omitted. Use `--max-ref-lines` to adjust the limit.

**Example**

A document containing:

```
@ref src/engine/store.rs#Store
```

Renders as:

````
```rust
pub struct Store { ... }
```
````

Unresolvable refs render as:

```
> [unresolved: src/engine/store.rs#Store]
```

</details>

<details>
<summary><h2>Configuration</h2></summary>

`lazyspec init` creates a `.lazyspec.toml` in your project root with four built-in document types:

```toml
[directories]
rfcs = "docs/rfcs"
adrs = "docs/adrs"
stories = "docs/stories"
iterations = "docs/iterations"

[templates]
dir = ".lazyspec/templates"

[naming]
pattern = "{type}-{n:03}-{title}.md"
```

### Custom Types

Instead of `[directories]`, you can define types explicitly with `[[types]]`. This lets you rename the defaults, add new types, or set custom prefixes and icons used in the TUI.

```toml
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
icon = "●"

[[types]]
name = "spec"
plural = "specs"
dir = "docs/specs"
prefix = "SPEC"
icon = "◆"
```

### Validation Rules

Validation rules define structural constraints between document types. Two shapes are supported:

- `parent-child` -- the child type must link to a parent type via a given relationship.
- `relation-existence` -- documents of a given type must have at least one relationship.

```toml
[[rules]]
shape = "parent-child"
name = "stories-need-rfcs"
child = "story"
parent = "rfc"
link = "implements"
severity = "warning"

[[rules]]
shape = "relation-existence"
name = "adrs-need-relations"
type = "adr"
require = "any-relation"
severity = "error"
```

### Numbering

Document numbers are assigned automatically during `create`. Three strategies are available per type:

| Strategy      | Behaviour                                                                                 |
| ------------- | ----------------------------------------------------------------------------------------- |
| `incremental` | Next sequential integer from existing files (default)                                     |
| `sqids`       | Short hash-like IDs derived from a timestamp, configured via `[numbering.sqids]`          |
| `reserved`    | Reserves numbers on a git remote before creating files, preventing distributed collisions |

Reserved numbering uses git custom refs (`refs/reservations/*`) to coordinate across branches. It wraps either incremental or sqids formatting with an atomic push-based lock, so two people never get the same number.

```toml
[[types]]
name = "rfc"
prefix = "RFC"
numbering = "reserved"

[numbering.reserved]
remote = "origin"        # default
format = "incremental"   # or "sqids"
max_retries = 5          # push retry attempts before failing
```

If the remote is unreachable, `create` fails rather than silently falling back. Use `lazyspec reservations prune` to clean up refs for documents that have been created.

### Orchestration

The `[orchestration]` section configures defaults for agent-driven workflows -- which user logins count as agents, and what document type is the unit of claimable work.

```toml
[orchestration]
agent_users = ["claude-bot"]
claim_type = "story"           # default
poll_interval_ms = 30000        # default (tick cadence)
max_concurrent_agents = 4       # default (concurrent agent cap)
active_statuses = ["todo", "in-progress"]  # default (eligible doc statuses)
handoff_states = ["in-review"]  # default (statuses that release without removing the worktree)
heartbeat_interval_ms = 300000  # default (5 minutes; daemon-side lease heartbeat)
metadata_push_interval_ms = 30000  # default (batched lease fetch window)
stall_timeout_ms = 300000       # default (5 minutes; tool_use suspends this timer)
max_turns = 20                  # default (continuation cap on clean exits)
max_failure_attempts = 5        # default (failure cap; stall/turn/abnormal/hook share counter)
max_retry_backoff_ms = 300000   # default (5 minutes; ceiling on exponential failure backoff)
continuation_delay_ms = 1000    # default (delay before re-invoking claude on clean exit)

[orchestration.runtime]
claude_binary = "claude"      # default
allowed_tools = ""             # default (comma-separated list passed to claude --allowedTools)
turn_timeout_ms = 600000       # default (per-turn hard wall; NOT suspended by tool_use)

[orchestration.hooks.after_create]
script = "scripts/after-create.sh"
timeout_ms = 60000             # default

[orchestration.hooks.before_run]
script = "scripts/before-run.sh"

[orchestration.hooks.after_run]
script = "scripts/after-run.sh"

[orchestration.hooks.before_remove]
script = "scripts/before-remove.sh"
```

- `agent_users` -- logins eligible to be picked up by the daemon. The first entry is the default for `lazyspec assign` when `--user` is omitted.
- `claim_type` -- document type the daemon claims as a unit of work. Defaults to `story`.
- `poll_interval_ms` -- tick-loop polling cadence in milliseconds. Defaults to `30000` (30 seconds).
- `max_concurrent_agents` -- maximum number of agents the daemon will run in parallel. Defaults to `4`.
- `active_statuses` -- document statuses eligible for dispatch. Defaults to `["todo", "in-progress"]`.
- `handoff_states` -- statuses that, when observed, kill the agent and release the lease but leave the worktree in place for operator follow-up. Defaults to `["in-review"]`. Anything outside `active_statuses` and `handoff_states` is treated as terminal: the worktree is removed.
- `heartbeat_interval_ms` -- daemon-side lease heartbeat cadence in milliseconds. Defaults to `300000` (5 minutes).
- `metadata_push_interval_ms` -- window in milliseconds for batched `git fetch refs/lazyspec/leases/*` and for push+fetch of `refs/lazyspec/agents/*` (per-session agent metadata). Lease freshness, metadata push, and metadata fetch all ride this single cadence rather than per-tick. Defaults to `30000` (30 seconds).
- `stall_timeout_ms` -- max idle time between agent stream-json events before the daemon kills the agent for retry. Suspended while a `tool_use` is in flight. Defaults to `300000` (5 minutes).
- `max_turns` -- continuation cap. After each clean exit (`code == 0`) with the doc still in an active status, the daemon re-invokes claude in the same workspace. Once `attempt > max_turns`, the daemon emits a `failed` event with reason `max_turns`, releases the lease, and stops. Defaults to `20`.
- `max_failure_attempts` -- failure cap. Stalls, turn timeouts, abnormal exits, and hook failures share one `failure_attempt` counter; once `failure_attempt > max_failure_attempts`, the daemon emits a `failed` event with reason `max_failure_attempts`, releases the lease, and stops. Defaults to `5`.
- `max_retry_backoff_ms` -- ceiling on exponential failure backoff. Retry delay is `min(10000 * 2^(n-1), max_retry_backoff_ms)`. Defaults to `300000` (5 minutes).
- `continuation_delay_ms` -- delay before re-invoking claude after a clean exit, in milliseconds. Defaults to `1000` (1 second).
- `runtime.claude_binary` -- path to the `claude` CLI the daemon invokes. Defaults to `claude` (looked up on `PATH`).
- `runtime.allowed_tools` -- comma-separated tool allowlist forwarded to `claude --allowedTools`. Empty string (default) means no restriction.
- `runtime.turn_timeout_ms` -- hard wall on a single agent turn, in milliseconds. NOT suspended by `tool_use`. Defaults to `600000` (10 minutes).
- `hooks.<point>.script` -- shell script invoked at the lifecycle point. Each `<point>` (`after_create`, `before_run`, `after_run`, `before_remove`) is an optional sub-table; omit a sub-table to skip that hook.
- `hooks.<point>.timeout_ms` -- per-hook timeout in milliseconds. Defaults to `60000` (60 seconds). `before_*` hooks are fatal (non-zero exit aborts the lifecycle step); `after_*` hooks are non-fatal (failures are logged but the daemon proceeds).

### Templates

Place markdown templates in the templates directory (`.lazyspec/templates/` by default). When creating a document, lazyspec uses the template matching the document type name (e.g. `rfc.md`, `story.md`).

</details>

## Development

### Nix (recommended)

The repo includes a Nix flake that provides the full toolchain. With [direnv](https://direnv.net/) installed:

```sh
direnv allow
```

Or enter the dev shell manually:

```sh
nix develop
```

This gives you cargo, clippy, rustfmt, and rust-analyzer at pinned versions.

To run all checks (clippy, tests, formatting):

```sh
nix flake check
```

### Without Nix

```sh
cargo build
cargo test
```
