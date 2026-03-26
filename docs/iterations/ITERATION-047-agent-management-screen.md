---
title: Agent management screen
type: iteration
status: accepted
author: agent
date: 2026-03-09
tags: []
related:
- implements: STORY-052
---




## Changes

Adds a persistent agent tracking system and a new TUI screen for viewing agent history. Agent metadata is stored under `~/.lazyspec/agents/` as JSON files. The spawner is refactored to generate a session ID per agent, write metadata to disk, and update status on completion. A new `Agents` view mode shows the history list with live status updates.

### Task 1: Agent record model and persistence

**ACs addressed:** AC2 (status tracking), AC5 (status updates)

**Files:**
- Modify: `src/tui/agent.rs`

**What to implement:**

Define an `AgentRecord` struct:
@ref src/tui/agent.rs#AgentRecord@9e60900f6d7b3a0dc4fdfa4f7aad3ec2c3ee8c92

@ref src/tui/agent.rs#AgentStatus@9e60900f6d7b3a0dc4fdfa4f7aad3ec2c3ee8c92

Add persistence functions:
- `agent_history_dir() -> PathBuf` -- returns `~/.lazyspec/agents/`, creating it if needed
- `save_record(record: &AgentRecord) -> Result<()>` -- writes `{session_id}.json` to the history dir
- `load_all_records() -> Result<Vec<AgentRecord>>` -- reads all JSON files from the history dir, sorted by `started_at` descending
- `update_record_status(session_id: &str, status: AgentStatus) -> Result<()>` -- reads, updates, rewrites the JSON file

Use `serde` for serialization. Keep it simple: one file per agent run, named by session ID.

**How to verify:** `cargo test agent_record` -- unit tests for round-trip serialize/deserialize and load_all ordering.

### Task 2: Refactor AgentSpawner to track records

**ACs addressed:** AC2 (status per agent), AC5 (live status updates)

**Files:**
- Modify: `src/tui/agent.rs`

**What to implement:**

Replace `children: Vec<Child>` with:
@ref src/tui/agent.rs#AgentSpawner@9e60900f6d7b3a0dc4fdfa4f7aad3ec2c3ee8c92

- `new()` calls `load_all_records()` to populate `records` on startup
- `spawn()` generates a UUID via `uuid::Uuid::new_v4()`, passes `--session-id <uuid>` to the `claude` command, creates an `AgentRecord` with `Running` status, calls `save_record()`, pushes to both `running` and `records`
- `poll_finished()` iterates `running`, calls `try_wait()`. On exit: determine `Complete` vs `Failed` from exit code, call `update_record_status()`, update the in-memory record in `records`, remove from `running`
- `active_count()` returns `running.len()`

Add `uuid` crate to `Cargo.toml`.

**How to verify:** `cargo test agent_spawner` -- test that spawn creates a record file, poll_finished updates status.

### Task 3: Agents view mode and screen state

**ACs addressed:** AC1 (navigate to agents screen), AC4 (empty state)

**Files:**
- Modify: `src/tui/app.rs`

**What to implement:**

Add `Agents` variant to `ViewMode` enum. Update `next()` to include it in the cycle (after `Graph`, before `Types`). Update `name()` to return `"Agents"`.

Add to `App`:
```rust
pub agent_selected_index: usize,
```

When entering `Agents` mode (in `cycle_mode` or wherever mode transitions happen), refresh `agent_spawner.records` by calling `load_all_records()` so we pick up any externally-created records.

Handle keys in agents mode:
- `Up`/`Down` (`j`/`k`): navigate the agent list, updating `agent_selected_index`
- `e`: set `editor_request` to the selected agent's `doc_path` (opens the target document in `$EDITOR`)
- `r`: run `claude --resume <session_id>` using the same terminal handoff pattern as `editor_request` (add a `resume_request: Option<String>` field to `App`, handled in the event loop like `editor_request`)
- Backtick: cycle to next mode (already handled)

**How to verify:** `cargo test agents_view` -- test mode cycling includes Agents, key handling updates selected index.

### Task 4: Render agents screen

**ACs addressed:** AC1 (list display), AC2 (status display), AC4 (empty state), AC5 (live updates)

**Files:**
- Modify: `src/tui/ui.rs`

**What to implement:**

Add `draw_agents_screen(f, app)` function. Call it from the main `draw()` when `view_mode == ViewMode::Agents`.

Layout: full-width list with columns for status icon, document title, action, and timestamp. Use colored status indicators:
- Running: yellow spinner/dot
- Complete: green checkmark
- Failed: red cross

When `app.agent_spawner.records.is_empty()`, render a centered message: "No agents have been invoked yet. Press `a` on a document to start one."

Highlight the selected row using the same reversed style as other list views. Show a footer hint: `e: open document  r: resume session  `: switch view`.

Live updates come for free since `poll_finished()` runs every tick and the screen redraws each loop iteration.

**How to verify:** Manual visual check via `cargo run`. Automated tests cover state only (Task 3).

### Task 5: Resume session in event loop

**ACs addressed:** AC1 (AC3 equivalent -- view/interact with agent)

**Files:**
- Modify: `src/tui/mod.rs`

**What to implement:**

Add handling for `app.resume_request` in the event loop, following the `editor_request` pattern:
1. Check `if let Some(session_id) = app.resume_request.take()`
2. Exit raw mode and alternate screen
3. Run `Command::new("claude").args(["--resume", &session_id]).status()`
4. Re-enter raw mode and alternate screen
5. Reload store (the agent may have modified documents)

Place this alongside the existing `editor_request` block.

**How to verify:** `cargo test resume_request` -- test that setting `resume_request` triggers the command (mock Command in test).

### Task 6: Tests

**ACs addressed:** AC1, AC2, AC4, AC5

**Files:**
- Create: `tests/tui_agent_management_test.rs`

**What to implement:**

Using `TestFixture` and the existing test patterns:

1. `test_agents_view_mode_in_cycle` -- cycle through modes, assert `Agents` appears
2. `test_agent_record_persistence` -- create an `AgentRecord`, save it, load all, assert it round-trips correctly
3. `test_agent_record_status_update` -- save a Running record, update to Complete, reload, assert status changed
4. `test_agents_screen_empty_state` -- enter Agents mode with no records, assert `agent_spawner.records.is_empty()`
5. `test_agents_screen_navigation` -- add records, press `j`/`k`, assert `agent_selected_index` changes
6. `test_agents_screen_r_key_sets_resume` -- select an agent, press `r`, assert `resume_request` is `Some(session_id)`
7. `test_agents_screen_e_key_opens_doc` -- select an agent, press `e`, assert `editor_request` is `Some(doc_path)`

**How to verify:** `cargo test tui_agent_management`

## Test Plan

| Test | AC | What it verifies | Tradeoffs |
|------|-----|-----------------|-----------|
| `test_agents_view_mode_in_cycle` | AC1 | Mode is reachable via backtick cycling | Fast, isolated |
| `test_agent_record_persistence` | AC2 | Records serialize/deserialize to disk correctly | Uses temp dir, slightly slower than pure unit |
| `test_agent_record_status_update` | AC2, AC5 | Status transitions persist to disk | Same as above |
| `test_agents_screen_empty_state` | AC4 | Empty records list is handled | Fast, state-only |
| `test_agents_screen_navigation` | AC1 | j/k keys move selection | Fast, state-only |
| `test_agents_screen_r_key_sets_resume` | AC1 | Resume keybinding wires up correctly | State-only, doesn't test actual claude invocation |
| `test_agents_screen_e_key_opens_doc` | AC1 | Editor keybinding targets the agent's document | State-only |

AC5 (live status updates) is covered implicitly -- `poll_finished()` already runs every 100ms tick and the test for status updates confirms the in-memory + on-disk state stays in sync. No explicit UI refresh test needed since the draw loop is unconditional.

## Notes

- AC3 from the Story says "open agent output in $EDITOR". We're reinterpreting this as two actions: `e` opens the target document, `r` resumes the Claude session. This better matches the user's intent since output isn't captured to disk.
- The `uuid` crate needs to be added to `Cargo.toml` with the `v4` feature.
- `~/.lazyspec/agents/` is a user-global directory, not project-scoped. Agent history is visible regardless of which project the TUI is running in. If project-scoping is desired later, the `agent_history_dir()` function is the single point to change.
