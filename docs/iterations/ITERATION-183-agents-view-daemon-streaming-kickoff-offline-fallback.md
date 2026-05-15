---
title: 'Agents view: daemon streaming, kickoff, offline fallback'
type: iteration
status: accepted
author: agent
date: 2026-05-15
tags: []
related:
- implements: STORY-122
---

## Scope

Replace legacy `draw_agents_screen` (RFC-036 era, `AgentSpawner` records from `.lazyspec/agent-history/*.json`) with daemon-driven view per RFC-041. Live socket subscription via existing `ReconnectingSubscriber` (`src/engine/ipc/client.rs`); offline fallback via `read_agent_metadata` (`src/engine/agent_metadata.rs`). Two-panel layout, status bar, manual-kickoff picker that does `set_assignees` + `Kick`.

Out: daemon-side anything (slices 1-6, 8 done). Legacy `tui/agent.rs` `AgentSpawner` + agent_dialog flow (one-shot spawn on `a` keybind) untouched -- separate feature surface.

## Changes

1. **Engine seam: subscriber handle factory.** (AC3, AC7)
   - File: `src/engine/ipc/client.rs` (add) or new `src/engine/ipc/mod.rs` export.
   - Add `pub fn default_subscriber(socket_path: PathBuf) -> ReconnectingSubscriber<SocketConnector, ThreadSleeper>` constructor returning the production wiring. Backoff: base 250ms, cap 5s (matches RFC-041 reconnect spirit, no new config).
   - Reason: TUI must not construct `SocketConnector` directly (dictum 3: TUI -> engine only). Engine owns the wiring; TUI calls `.events()` to get the `Receiver<DaemonMessage>`.

2. **AgentsViewState struct.** (AC1, AC2, AC3, AC4, AC6, AC7)
   - File: `src/tui/state/agents.rs` (new). Module decl in `src/tui/state.rs`.
   - Fields:
     - `snapshots: BTreeMap<String, AgentSnapshot>` keyed by `session_id` (BTreeMap → stable row order).
     - `output: HashMap<String, String>` session_id → concat stream-text buffer (bounded; trim at 64KiB per session).
     - `selected: usize` row index into `snapshots` keys.
     - `connection: ConnectionState` enum `Connected | Reconnecting | Offline`.
     - `last_event_at: HashMap<String, Instant>` for elapsed-time render.
   - Methods: `apply(DaemonMessage)`, `load_offline(Vec<AgentMetadata>)`, `selected_session() -> Option<&str>`, `aggregate_tokens() -> (u64, u64)`, `counts_by_status() -> HashMap<AgentStatus, u32>`.
   - Pure logic, no I/O. Unit-testable.

3. **Wire subscriber into App.** (AC3, AC7)
   - File: `src/tui/state/app.rs`.
   - Add `pub agents_view: AgentsViewState` field. Drop `agent_selected_index` (subsumed).
   - Add `AppEvent::AgentsDaemon(DaemonMessage)` variant.
   - In main event loop (likely `src/main.rs` or `src/tui.rs`): spawn a thread that owns the `Receiver<DaemonMessage>` from `default_subscriber(...).events()` and forwards via the existing app-event mpsc as `AppEvent::AgentsDaemon`. Connection state transitions emitted as synthetic `DaemonMessage::Error { message: "reconnecting" }` are not used; instead, the subscriber thread sends a separate `AppEvent::AgentsConnectionChanged(ConnectionState)` when it detects connect/disconnect (extend `ReconnectingSubscriber` to expose state, or wrap and observe). Prefer: extend `ReconnectingSubscriber` with an optional `state_tx` callback channel; one new public method `with_state_channel(self, tx)`.
   - Verify socket path comes from config: `[orchestration].socket_path` or default `.lazyspec/daemon.sock`. Check `src/engine/config.rs` for existing field; add if missing (one-line addition).

4. **Two-panel render.** (AC1, AC2)
   - File: `src/tui/views/panels.rs`. Replace existing `draw_agents_screen` body (line 1427).
   - Horizontal split: left 40%, right 60%.
   - Left: `Table` of `app.agents_view.snapshots`. Columns: icon (3), session-id-short (10), doc_id (12), elapsed (8), tokens (`{in}/{out}`, 16). Status icon mapping: `Running`=`●`/yellow, `Idle`=`○`/gray, `Completed`=`✔`/green, `Failed`=`✘`/red, `Crashed`=`!`/red. Reuse vocabulary from `engine::agent_metadata::AgentStatus`.
   - Right: `Paragraph` of `app.agents_view.output[selected_session]` with wrap + scroll-to-bottom. Empty state: "No output yet" dimmed.
   - Footer line below panels (same area): hotkey legend incl. new kickoff key.

5. **Status bar.** (AC4)
   - File: `src/tui/views/status_bar.rs` or component plumbing.
   - When `ViewMode::Agents`, add status-bar components: daemon connection (green/yellow/red dot + label `connected|reconnecting|offline`), counts (e.g. `2 running, 1 idle`), aggregate tokens (`in: 1.2k / out: 3.4k`).
   - Implementation: extend `StatusBarComponent` enum (see `src/tui/state/app.rs::status_bar_components`) with view-aware components, or compute inline in `draw_status_bar` keyed on `app.view_mode`.

6. **Manual-kickoff picker.** (AC5)
   - File: `src/tui/views/overlays.rs` (new `draw_kickoff_picker`); `src/tui/state/forms.rs` (new `KickoffPicker` state).
   - Trigger: keybind `n` (new) in `ViewMode::Agents` (see `src/tui/views/keys.rs` for binding map).
   - Picker contents: filtered list of documents matching `config.orchestration.claim_type` (default `story`) with `status ∈ active_statuses` and no overlap with `agent_users` already in assignees. Source: existing `Store::load` already cached in app state — check `app.docs`.
   - On select: call `engine::assignees::set_assignees(&path, &fs, vec![first_agent_user.clone()])` (additive merge; verify `set_assignees` semantics — read source). If subscriber connection state is `Connected`, send `ClientMessage::Kick` via a fresh `SocketConnector::connect()` (cheap: one-shot send). If offline, skip kick; assignment persists.
   - The send-kick path needs a writable handle to the socket. Subscriber owns its connection (read loop). Simplest path: open a second short-lived `UnixStream` for the kick message. Verify this is allowed by daemon IPC; if daemon serializes one client at a time, need different approach — read `src/engine/ipc/handler.rs` first.

7. **Offline fallback.** (AC6)
   - File: `src/tui/state/agents.rs` + `src/main.rs` (or wherever app init runs).
   - On view entry (`ViewMode::Agents` activation), if `connection != Connected`, call `engine::agent_metadata::read_agent_metadata` for all session ids under `refs/lazyspec/agents/*`. Need a `list_agent_sessions` helper — check `src/engine/agent_metadata.rs`; add a `pub fn list_sessions<G: GitRefOps>(git: &G) -> Result<Vec<String>>` if missing.
   - Populate `AgentsViewState::snapshots` + read-only flag. Status bar shows `offline` indicator (red dot, label `offline (history)`).
   - When daemon comes online (state transitions `Offline → Reconnecting → Connected`), drop the snapshot map and rely on socket stream.

8. **Reconnect resilience.** (AC7)
   - File: `src/engine/ipc/client.rs`.
   - Confirm `ReconnectingSubscriber::events()` already drains reconnect-loop panics into the receiver. Read the impl; if any `unwrap()` or panic path crashes the thread, replace with a graceful loop continuation that emits a state transition (`Reconnecting`).
   - No view-side panic: stream gap leaves `output` buffers intact; new events append.

9. **Cleanup.** Remove `agent_selected_index`, `agent_spawner` references from agents-view code path (keep `agent_spawner` only for the `agent_dialog` flow). Verify the legacy spawner-driven view fallback is gone — `cfg(feature = "agent")` gating preserved.

10. **README + help overlay.** Update `README.md` CLI/keybind section. Update `src/tui/views/overlays.rs::draw_help_overlay` agents-view block to list new `n` kickoff binding.

## Test Plan

Tests are unit-level where possible (`AgentsViewState` is pure), integration-level via ratatui `TestBackend` for render, fake `Connector` + `Sleeper` for subscriber (already a trait seam per dictum 4). No real socket, no real git.

| AC | Test | Location | Verifies |
|----|------|----------|----------|
| 1 | `agents_view_renders_two_panels` | `tests/tui_agents_view.rs` | TestBackend renders; left + right regions have distinct content (table on left, paragraph on right). |
| 2 | `agent_row_has_icon_doc_id_elapsed_tokens` | `src/tui/views/panels.rs` cfg(test) | Unit on cell builder fn (extracted like existing `doc_row_cells_for_test`). Given `AgentSnapshot{status=Running, doc_id=STORY-1, elapsed=125s, in=100, out=200}`, cells contain `●`, `STORY-1`, `2m05s` (or chosen format), `100/200`. |
| 3 | `selected_agent_output_buffer_updates_on_event` | `src/tui/state/agents.rs` cfg(test) | `state.apply(AgentEvent::Text{delta:"hello"})` for selected session appends to `output[session]`; non-selected session also accumulates but is not visible. Behavioral: assert buffer contents post-apply. |
| 3 | `streaming_renders_incremental_text` | `tests/tui_agents_view.rs` | Two applies + render; right panel contains concatenated text. |
| 4 | `status_bar_aggregates_counts_and_tokens` | `src/tui/state/agents.rs` cfg(test) | 3 snapshots (Running, Running, Failed) + token sums → `(running:2, failed:1)`, `(sum_in, sum_out)`. |
| 4 | `status_bar_shows_connection_label` | `tests/tui_agents_view.rs` | Render with `connection=Connected` shows `connected`; with `Offline` shows `offline`. |
| 5 | `kickoff_picker_assigns_doc_and_sends_kick` | `tests/tui_agents_kickoff.rs` | Fake `FileSystem` + fake `Connector`. Open picker, select doc, press enter. Assert: `set_assignees` mutated the doc's frontmatter (read back); `Connector::connect` was called and `ClientMessage::Kick` written. |
| 5 | `kickoff_when_offline_writes_but_skips_kick` | same | Fake connector returns `Err`. Assert frontmatter mutated, no kick attempted (or attempted and absorbed silently). |
| 6 | `offline_fallback_loads_agent_refs` | `tests/tui_agents_offline.rs` | Fake `GitRefOps` returns 2 session refs; view entered with `connection=Offline`; assert snapshots populated, read-only flag set, offline indicator rendered. |
| 7 | `reconnect_after_socket_drop_no_panic` | `tests/ipc_client_reconnect.rs` (extend existing if present) | Fake `Connector` returns `Ok` then `Err` then `Ok`; fake `Sleeper` zero-delay. Drive events through; assert subscriber thread does not panic, state transitions emitted (`Connected → Reconnecting → Connected`), receiver still alive after drop. |
| 7 | `view_state_survives_reconnect` | `src/tui/state/agents.rs` cfg(test) | Apply events, simulate `ConnectionChanged(Reconnecting)`, apply more events post-reconnect → output buffers contain both pre and post text. |

Test properties tradeoff:

- **Behavioral vs structural**: AC2 cell-builder unit test asserts on `Cell::Debug` strings (matches existing `doc_row_cells_for_test` pattern); coupled to ratatui's Debug fmt. Accept the coupling — same precedent in `src/tui/views.rs::tests` already. Alternative is TestBackend pixel assertions which are more brittle.
- **Determinism**: elapsed time render uses `Instant::now() - last_event_at`. Test seam needed. Inject a `Clock` trait (engine seam, dictum 4) only if a second use shows up; for v1, factor `format_elapsed(d: Duration) -> String` as a pure fn and test that directly; render call sites compute `Instant::now().duration_since(last_event)` and pass in. No new trait.
- **Isolation**: kickoff test uses `TempDir`. Subscriber tests use fakes for `Connector` + `Sleeper`.

## Notes

**Existing infra confirmed:**
- `src/engine/ipc/client.rs::ReconnectingSubscriber<C: Connector, S: Sleeper>` — reuse. Has backoff schedule.
- `src/engine/ipc/protocol.rs::{ClientMessage, DaemonMessage, AgentSnapshot}` — wire types ready.
- `src/engine/agent_metadata.rs::{AgentMetadata, AgentStatus, read_agent_metadata, GitRefAgentMetadata}` — offline read path ready. May need `list_sessions` helper (verify before adding).
- `src/engine/assignees.rs::set_assignees` — kickoff write path. Signature must be verified before task 6.

**Open items to verify during build (do not pre-emptively code):**
- a. Does `ReconnectingSubscriber` already expose connection-state transitions? If not, extend with `with_state_channel`. (Task 3, 8.)
- b. Does daemon `handler.rs` accept multiple concurrent client connections (one for subscribe, one for one-shot Kick)? If single-client, kickoff must reuse the subscribe socket — requires shared write half. (Task 6.)
- c. Does `config.orchestration` have a `socket_path` field? If not, default `.lazyspec/daemon.sock` per RFC-041 with one-field config addition. (Task 3.)
- d. `list_agent_sessions` helper existence in `agent_metadata.rs`. (Task 7.)

**Dictum compliance:**
- Dictum 3: TUI calls `engine::ipc::default_subscriber`, `engine::assignees::set_assignees`, `engine::agent_metadata::read_agent_metadata` directly. No CLI dependency.
- Dictum 4: `Connector`, `Sleeper`, `GitRefOps`, `FileSystem` traits already exist as seams. No new traits unless a second concrete use appears.
- Dictum 6: One concrete use of `Clock` (elapsed time); use pure `format_elapsed` instead of a trait.

**Status icon vocabulary:** RFC-041 says align with daemon agent state machine. `engine::agent_metadata::AgentStatus` already defines the set; use it directly. No new enum.

**Risk:** Task 6 (kickoff send-kick path) depends on daemon handler concurrency. If daemon serializes clients, design has to change to share the subscriber's write half. Confirm before implementing the kickoff path.

