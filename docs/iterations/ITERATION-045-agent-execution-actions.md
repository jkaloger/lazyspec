---
title: Agent execution actions
type: iteration
status: accepted
author: agent
date: 2026-03-08
tags: []
related:
- implements: docs/stories/STORY-051-agent-invocation-from-tui.md
---



## Changes

Implements agent execution for the TUI agent dialog (Group B: AC2, AC3, AC5, AC6, AC10). Assumes Group A (dialog UI, keybinding, rendering) is already complete.

### Task 1: Agent spawning module

**ACs addressed:** AC10, AC6
**Files:**
- Create: `src/tui/agent.rs`
- Modify: `src/tui/mod.rs` (add module declaration)
- Test: `tests/tui_agent_test.rs`

**What to implement:** Define an `AgentSpawner` struct that wraps headless Claude invocation. Method `spawn(prompt: &str, doc_path: &Path) -> Result<Child>` uses `Command::new("claude").args(["--print", "-p", prompt]).spawn()` for non-blocking execution. The struct holds `Vec<Child>` for tracking running processes. Add a `poll_finished(&mut self)` method to reap completed processes. Claude-only (AC10) with no abstraction over provider yet.

**How to verify:** `cargo test tui_agent` — mock Command execution, assert spawn is non-blocking.

### Task 2: Expand document action

**ACs addressed:** AC2
**Files:**
- Modify: `src/tui/app.rs` (handle expand selection from dialog)
- Modify: `src/tui/agent.rs` (add expand prompt builder)
- Test: `tests/tui_agent_test.rs`

**What to implement:** When the dialog yields `AgentAction::Expand`, read the document content via `std::fs::read_to_string`, build a prompt instructing Claude to flesh out the document, and call `AgentSpawner::spawn()`. The prompt includes the full document text and instructions to preserve frontmatter and expand sparse sections.

**How to verify:** `cargo test expand_action` — assert correct prompt is built and spawn is called.

### Task 3: Create children action

**ACs addressed:** AC3
**Files:**
- Modify: `src/tui/app.rs` (handle create-children selection)
- Modify: `src/tui/agent.rs` (add children prompt builder)
- Test: `tests/tui_agent_test.rs`

**What to implement:** When dialog yields `AgentAction::CreateChildren`, look up the document's type in `Config.rules` to find the child type via `ParentChild` rules. Build a prompt instructing Claude to generate child documents of that type, using `lazyspec create` commands. Pass the parent document content as context.

**How to verify:** `cargo test create_children_action` — assert child type is derived from config rules and prompt references it.

### Task 4: Custom prompt text input

**ACs addressed:** AC5
**Files:**
- Modify: `src/tui/app.rs` (add `AgentDialogState::TextInput` sub-state, handle char input and submit)
- Modify: `src/tui/ui.rs` (render text input field within dialog)
- Test: `tests/tui_agent_test.rs`

**What to implement:** When user selects "Custom prompt" in the dialog, transition to a `TextInput` sub-state with a `String` buffer. Handle character keys to build the prompt, Enter to submit. On submit, spawn Claude with the user's freeform prompt and the selected document as context.

**How to verify:** `cargo test custom_prompt` — simulate keystrokes, assert prompt buffer builds correctly and spawn receives the custom text.

### Task 5: Background execution integration

**ACs addressed:** AC6
**Files:**
- Modify: `src/tui/app.rs` (add `agent_spawner: AgentSpawner` field to `App`)
- Modify: `src/tui/mod.rs` (call `poll_finished()` in event loop)
- Test: `tests/tui_agent_test.rs`

**What to implement:** Add `AgentSpawner` as a field on `App`. After each event loop tick, call `agent_spawner.poll_finished()` to reap completed child processes. All agent actions from Tasks 2-4 route through this single spawner. The TUI event loop is never blocked because `spawn()` returns immediately.

**How to verify:** `cargo test background_agent` — spawn a mock process, assert TUI handle_key continues to work while process is "running".

### Task 6: Tests

**ACs addressed:** AC2, AC3, AC5, AC6, AC10
**Files:**
- Create: `tests/tui_agent_test.rs`

**What to implement:** Integration tests using `TestFixture`. Mock process spawning by injecting a test `AgentSpawner` that records calls instead of executing Claude. Test cases:
1. Expand action builds correct prompt from document content
2. Create-children derives child type from config ParentChild rules
3. Custom prompt captures user keystrokes and passes to spawn
4. Spawned processes don't block `handle_key` dispatch
5. Only Claude binary is invoked (no other providers)

**How to verify:** `cargo test tui_agent`

## Test Plan

- Mock `Command` execution via an `AgentSpawner` trait or test-only constructor that captures spawn calls without invoking Claude
- Text input flow tested by simulating `KeyCode::Char` sequences followed by `KeyCode::Enter`
- Background non-blocking verified by asserting `handle_key` returns immediately after spawn
- Child type derivation tested against fixture configs with known ParentChild rules
- All tests use `TestFixture` with `TempDir` for isolation

## Notes

Depends on Group A iteration (dialog UI infrastructure) being merged first. The `AgentAction` enum and `AgentDialog` struct referenced here will come from that iteration.
