---
title: AgentRunner trait and ClaudeP stream-json runner
type: iteration
status: accepted
author: agent
date: 2026-05-13
tags: []
related:
- implements: STORY-127
---

## In Scope

- `AgentRunner` trait. seal over norm events.
- `AgentContext` (workspace path, doc id, agent id, branch).
- `AgentHandle { pid, events: Receiver<AgentEvent>, cancel: Sender<()> }`.
- `AgentEvent` enum: 6 variants — `SessionStarted`, `Text{delta}`, `ToolCall{name,summary,status}`, `TurnCompleted{input_tokens,output_tokens}`, `SubprocessExited{code}`, plus internal as needed.
- `ToolStatus` enum: `Ok` / `Error` (terminal only).
- `ClaudeP` impl. spawn `claude -p --output-format stream-json`. stdout reader thread → parse → emit events. waitpid thread → exit event + close chan.
- JSON-lines parser. `serde_json::Value` shape. unknown record types = ignore. forward-compat.
- Cancel = SIGTERM to pid.
- Config `[orchestration.runtime]`: `claude_binary`, `allowed_tools`, `turn_timeout_ms`.

## Out of Scope

- worktree provisioning (group A).
- branch templating (group A).
- hook lifecycle (group C).
- tick loop / scheduling.
- prompt content / context assembly.
- retry / continuation policy.
- IPC streaming to clients.
- multi-turn state persistence in-process.

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

## Test Plan

- AC1: spawn via trait. assert handle exposes pid > 0, events recv (bounded wait), cancel send ok. fake binary = canned stream-json emitter script. drop handle → cancel propagates.
- AC2: feed fixture line `{"type":"session_start",...}` to `parse_record`. assert `Some(AgentEvent::SessionStarted)`. integration: spawn fake binary that prints session_start, recv from chan with 500ms timeout.
- AC3: fixture lines for assistant text deltas. assert ordered `Text{delta}` events (vec collect on recv). chunks A,B,C arrive A,B,C.
- AC4: fixture pair tool_use + tool_result. assert single `ToolCall{name,summary,status:Ok}`. error result variant → `status:Error`.
- AC5: fixture turn-complete record w/ token counts. assert `TurnCompleted{input,output}` matches.
- AC6: fake binary exits code N. assert recv → `SubprocessExited{code:N}`, next recv → `Disconnected` (chan closed).
- Predictive: unknown record `{"type":"future_thing"}` → `parse_record` returns `None`, no panic, stream continues.
- No sleeps. all waits bounded `recv_timeout`. fake binary = test helper rust bin in `tests/bin/` or shell that `cat`s fixture file.

## Changes

1. **AC1,2,3,4,5,6** — new module `src/engine/runner.rs`.
   - types: `AgentContext { workspace: PathBuf, doc_id: String, agent_id: String, branch: String }`.
   - `AgentHandle { pid: u32, events: crossbeam_channel::Receiver<AgentEvent>, cancel: crossbeam_channel::Sender<()> }`.
   - `AgentEvent` enum w/ 6 variants per AC2-6.
   - `ToolStatus { Ok, Error }`.
   - `pub trait AgentRunner { fn spawn(&self, ctx: AgentContext) -> Result<AgentHandle>; }`.
   - re-export from `src/engine.rs`.
   - verify: `cargo build`, `cargo clippy -- -D warnings`.

2. **AC2,3,4,5** — stream parser. `src/engine/runner/stream.rs` (or submodule of runner.rs).
   - `pub(crate) fn parse_record(line: &str) -> Option<AgentEvent>`.
   - `serde_json::from_str::<Value>(line).ok()?`. match `obj["type"]`. dispatch:
     - `"session_start"` → `SessionStarted`.
     - `"assistant"` / text delta record → `Text { delta }`.
     - `"tool_use"` + later `"tool_result"` need pairing — keep parser pure: emit `ToolCall` on `tool_result` arrival (carries name+status), summary from tool input snapshot. simpler: emit on combined record if stream-json provides one; else parser returns pair when `tool_result` seen and caller stitches. v1: emit single `ToolCall` per `tool_result` record using fields it carries.
     - `"turn_complete"` / `"result"` w/ usage → `TurnCompleted { input_tokens, output_tokens }`.
     - unknown / missing `type` → `None`.
   - unit-test fixture strings inline.
   - verify: `cargo test engine::runner::stream`, `cargo clippy`.

3. **AC1,2,3,4,5,6** — `ClaudeP` impl. `src/engine/runner/claudep.rs` (or inline in runner.rs).
   - `pub struct ClaudeP { pub binary: String, pub allowed_tools: String, pub turn_timeout_ms: u64 }`.
   - `impl AgentRunner for ClaudeP`. `spawn`:
     - `Command::new(&self.binary).arg("-p").arg("--output-format").arg("stream-json").arg("--allowedTools").arg(&self.allowed_tools).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()`.
     - capture `pid`. `crossbeam_channel::unbounded::<AgentEvent>()` for events, `bounded::<()>(1)` for cancel.
     - stdout reader thread: `BufReader::new(stdout).lines()`. each line → `parse_record` → send if `Some`.
     - cancel watcher thread: on recv → `nix::sys::signal::kill(pid, SIGTERM)` (or libc shim; check existing deps).
     - waitpid thread: `child.wait()` → send `SubprocessExited { code }` → drop sender (closes chan).
     - return `AgentHandle`.
   - verify: integration test w/ fake binary, `cargo clippy`.

4. **All ACs** — config plumbing `src/engine/config.rs`.
   - add `#[derive(Deserialize)] struct RuntimeConfig { claude_binary: String (default "claude"), allowed_tools: String (default ""), turn_timeout_ms: u64 (default 600_000) }`.
   - nest under existing `OrchestrationConfig` as `pub runtime: RuntimeConfig`.
   - serde defaults so existing configs stay valid.
   - verify: `cargo test engine::config`, `cargo clippy -- -D warnings`.

## Notes

- trait sealed over norm events per RFC-041 "Agent runtime protocol".
- unknown stream records = ignored. forward-compat w/ future Claude CLI fields.
- v1 = single concrete impl (ClaudeP). no other runners now.
- no cross-turn state in-process. each spawn = fresh subprocess.
- subprocess seam isolates non-determinism per dictum 4. trait makes test doubles cheap.
- tool_use/tool_result pairing: v1 emits `ToolCall` on `tool_result` arrival. stitching across separate records can land later if needed; story AC4 satisfied by single event carrying name+summary+terminal-status.
- cancel = SIGTERM. caller responsible for follow-up SIGKILL if SIGTERM ignored (out of scope here).
