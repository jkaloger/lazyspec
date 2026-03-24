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

## Acceptance Criteria

### AC: agent-dialog-opens-on-keypress

Given a document is selected in Types view
When the user presses `a`
Then a centered modal dialog appears listing available agent actions for that document

### AC: expand-document-spawns-agent

Given the agent dialog is open
When the user selects "Expand document" and presses Enter
Then a headless `claude` process is spawned with a prompt that instructs the agent to flesh out the document using the Edit tool, and the dialog closes

### AC: create-children-respects-config-rules

Given the agent dialog is open for a document whose type has a `ParentChild` rule in config
When the user selects "Create children" and presses Enter
Then a headless `claude` process is spawned with a prompt that instructs the agent to generate child documents of the configured child type

### AC: create-children-hidden-for-leaf-types

Given the agent dialog is open for a document whose type has no `ParentChild` rule mapping it as a parent
When the dialog renders
Then the "Create children" action is not present in the actions list

### AC: custom-prompt-text-input

Given the agent dialog is open
When the user selects "Custom prompt" and presses Enter
Then the dialog transitions to a text input field where the user can type a freeform prompt

### AC: custom-prompt-spawns-agent

Given the custom prompt text input is active and the user has typed a non-empty prompt
When the user presses Enter
Then a headless `claude` process is spawned with the user's text combined with the document content, and the dialog closes

### AC: spawn-uses-null-stdio

Given any agent action is triggered
When the `claude` subprocess is created
Then stdin, stdout, and stderr are all set to `Stdio::null()` so the TUI retains terminal control

### AC: spawn-persists-record

Given an agent is spawned
When the spawn completes
Then an `AgentRecord` JSON file is written to `~/.lazyspec/agents/{session_id}.json` with status `Running` and a UTC `started_at` timestamp

### AC: poll-updates-finished-agents

Given one or more agents are running
When `poll_finished` is called during the event loop tick
Then any process that has exited is detected via `try_wait`, its record status is updated to `Complete` or `Failed`, and a `finished_at` timestamp is set

### AC: agents-screen-displays-table

Given the user navigates to the Agents view mode via backtick
When agent records exist
Then a table is rendered showing status icon, truncated session ID, document title, action name, and start time for each record

### AC: agents-screen-empty-state

Given the user navigates to the Agents view mode
When no agent records exist
Then a centered message reads "No agents have been invoked yet. Press `a` on a document to start one."

### AC: resume-session-leaves-tui

Given the user is on the agents screen with a completed or failed agent selected
When the user presses `r`
Then the TUI leaves alternate screen, runs `claude --resume <session_id>` interactively, and re-enters alternate screen with a full store reload on return

### AC: resume-blocked-for-running-agents

Given the user is on the agents screen with a running agent selected
When the user presses `r`
Then no resume is triggered and the TUI remains in its current state

### AC: feature-gate-compiles-clean

Given the `agent` feature flag is disabled
When the project is compiled
Then no agent-related structs, view modes, key handlers, or rendering functions are included, and the TUI compiles without errors
