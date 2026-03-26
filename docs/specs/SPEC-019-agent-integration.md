---
title: "Agent Integration"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: [tui, agent, claude]
related:
  - implements: "docs/stories/STORY-051-agent-invocation-from-tui.md"
  - implements: "docs/stories/STORY-052-agent-management-screen.md"
  - implements: "docs/stories/STORY-053-custom-agent-prompts.md"
---

## Summary

The agent integration subsystem allows users to spawn headless Claude CLI sessions from within the TUI. It is gated behind the `agent` Cargo feature flag: all related structs, view modes, key handlers, and rendering functions are compiled conditionally via `#[cfg(feature = "agent")]`. When disabled, the TUI compiles and runs without any agent-related code paths.

## Agent Spawner

@ref src/tui/agent.rs#AgentSpawner

`AgentSpawner` owns a list of running child processes and their corresponding records. On construction it loads all persisted records from disk via `load_all_records`. The `spawn` method launches a new `claude` process with `Command::new("claude")`, passing `-p <prompt>`, `--session-id <uuid>`, and `--allowedTools Read,Edit,Write,Bash(lazyspec *)`. All three stdio handles (stdin, stdout, stderr) are set to `Stdio::null()`, so the subprocess runs fully headless and does not compete with the TUI for terminal access.

@ref src/tui/agent.rs#AgentRecord

Each spawn generates a UUID v4 session ID and creates an `AgentRecord` containing the session ID, document title, document path, action name, status, and timestamps. The record is immediately persisted to disk and pushed onto the in-memory `records` vec.

## Completion Polling

@ref src/tui/agent.rs#AgentStatus

The event loop calls `poll_finished` on every tick (approximately every 16ms). This method iterates the `running` vec and calls `try_wait()` on each `Child`. A successful exit maps to `AgentStatus::Complete`; a non-zero exit or an error maps to `AgentStatus::Failed`. Finished processes are removed from the `running` vec, and their on-disk records are updated with the new status and a `finished_at` timestamp via `update_record_status`.

## Record Persistence

Records are stored as individual JSON files in `~/.lazyspec/agents/`, named `{session_id}.json`. The directory is created lazily by `agent_history_dir`. `load_all_records` reads every `.json` file in the directory, skips any that fail to deserialize, and returns records sorted by `started_at` in descending order. `save_record` writes a pretty-printed JSON file. `update_record_status` reads, patches, and rewrites the file for a given session ID.

## Built-in Prompts

@ref src/tui/agent.rs#build_expand_prompt

The "Expand document" prompt instructs the agent to flesh out sparse sections of a document while preserving YAML frontmatter. It embeds the full document content and the file path, and directs the agent to use the Edit tool for in-place modification.

@ref src/tui/agent.rs#build_create_children_prompt

The "Create children" prompt instructs the agent to generate child documents using `lazyspec create {child_type}`. The child type is derived from the project's `ParentChild` validation rules in config. If no rule maps the selected document's type to a child type, the action is not offered.

## Custom Prompt Input

@ref src/tui/views/overlays.rs#draw_agent_dialog

When the user selects "Custom prompt" from the agent dialog, the dialog transitions to a text input state. The `AgentDialog` struct's `text_input` field switches from `None` to `Some(String::new())`. On submit, the user's text is combined with the document content into a prompt of the form "Here is the document:\n\n{content}\n\nUser request: {input}" and spawned as a new agent.

## Agent Dialog

@ref src/tui/state/forms.rs#AgentDialog

The agent dialog is a modal overlay activated by pressing `a` on a selected document in Types view. It populates its `actions` vec with "Expand document" and "Custom prompt" unconditionally, and adds "Create children" only when the document's type has a matching `ParentChild` rule in config. Navigation uses Up/Down arrows with wrapping. Esc closes the dialog. The dialog is rendered as a `List` widget with `REVERSED` highlight style.

## Agents Screen

@ref src/tui/views/panels.rs#draw_agents_screen

The agents screen is a dedicated `ViewMode::Agents` variant, reachable by cycling views with the backtick key. It renders a `Table` with columns for status icon, truncated session ID, document title, action, and start time. Status icons are: yellow `●` for running, green `✔` for complete, red `✘` for failed. When no records exist, a centered empty-state message is shown. A footer displays keybindings for `e` (open document), `r` (resume session), and backtick (switch view).

## Session Resume

@ref src/tui/views/keys.rs#handle_agents_key

Pressing `r` on a non-running agent record in the agents screen sets `resume_request` to that record's session ID. The event loop then leaves alternate screen, disables raw mode, runs `claude --resume <session_id>` as a blocking subprocess, and re-enters alternate screen on return. After resume, the store is fully reloaded to pick up any changes the agent made.

## Key Handling

@ref src/tui/views/keys.rs#handle_agent_dialog_key

Agent key handling is split across three `#[cfg(feature = "agent")]` methods on `App`. `handle_agent_dialog_key` dispatches the action menu (Up/Down/Enter/Esc). If the selected action is "Custom prompt", it transitions to text input mode rather than spawning immediately. `handle_agent_text_input_key` handles character input, backspace, Esc (back to menu), and Enter (submit). `handle_agents_key` manages the agents screen with j/k navigation, Ctrl-d/u half-page jumps, `e` to open the document in `$EDITOR`, `r` to resume, and `q` to quit.
