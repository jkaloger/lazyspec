---
title: Interactive agent run mode with configurable command
type: story
status: draft
author: jkaloger
date: 2026-06-18
tags: []
related:
- implements: RFC-046
---

## Context

Headless agents run `claude -p` in the background, but there is no way to open a live, interactive agent session on a document -- the way the TUI `e` command hands the terminal to `$EDITOR`, but for a live agent (`claude`, `opencode`, `pi`, or a custom shell/tmux command). The interactive tool varies by machine and project, so it is configured once as a global `[agents] interactive` shell command in toml rather than hardcoded. This story gives the template `mode: interactive` field its behaviour: selecting such a template suspends the TUI and hands over the terminal to the configured command, mirroring `run_editor`. See ADR-017.

## Acceptance Criteria

### AC1: Interactive command parsed from config

**Given** a `.lazyspec.toml` with an `[agents]` block setting `interactive = 'claude "$LAZYSPEC_PROMPT"'`
**When** config is loaded
**Then** the `interactive` shell command is available to the engine as `AgentsConfig.interactive`

### AC2: Selecting an interactive template hands over the terminal

**Given** `[agents] interactive` is set and an interactive-mode template is selected in the agent dialog for a document
**When** the action is dispatched
**Then** the TUI suspends (leaves the alternate screen, disables raw mode), and the configured command runs via `bash -lc` attached to the inherited terminal, blocking until it exits

### AC3: Rendered prompt and document path exported to the command

**Given** an interactive template whose body has been rendered for the selected document
**When** the configured command is launched
**Then** the rendered template body is exported as `$LAZYSPEC_PROMPT` and the document's path as `$LAZYSPEC_DOC_PATH` in the child's environment, and the command references them

### AC4: Screen restored after the session exits

**Given** an interactive agent session is running with the terminal handed over
**When** the session process exits
**Then** the TUI restores (re-enables raw mode, re-enters the alternate screen, clears) and drains buffered stdin, returning the user to where they were

### AC5: Unset interactive command means interactive templates are not offered

**Given** `[agents] interactive` is unset
**When** the agent dialog is opened for a document whose resolved templates include interactive-mode templates
**Then** those interactive templates are neither listed nor runnable, and no headless behaviour is affected

### AC6: Custom shell/tmux command works

**Given** `[agents] interactive` is set to a custom shell invocation such as `tmux new-window claude "$LAZYSPEC_PROMPT"`
**When** an interactive template is selected and dispatched
**Then** the command runs via `bash -lc` with `$LAZYSPEC_PROMPT` / `$LAZYSPEC_DOC_PATH` available, so the custom wrapper launches the session

### AC7: Interactive runs leave no AgentRecord and ignore allowed_tools

**Given** an interactive template (with or without an `allowed_tools` value) is run to completion
**When** the session exits
**Then** no `AgentRecord` is written (matching the `e` editor command), and the template's `allowed_tools` is ignored because the configured command owns its own tool policy

## Scope

### In Scope

- A global `[agents]` config block with `interactive: Option<String>` (a `bash -lc` shell command string), e.g. `claude "$LAZYSPEC_PROMPT"`, `opencode -p "$LAZYSPEC_PROMPT"`, `pi`, `tmux new-window claude "$LAZYSPEC_PROMPT"`
- Giving the template `mode: interactive` field its run behaviour (Story 2 only parses the field)
- Building the interactive `Command` in the engine (program, args, env) with the rendered template body exported as `$LAZYSPEC_PROMPT` and the document path as `$LAZYSPEC_DOC_PATH`; the engine never touches terminal state (dictum 3)
- Wiring the TUI dialog to dispatch interactive-mode selections through the suspend/run/restore sequence -- leave alternate screen, disable raw mode, run the child to exit, re-enter, drain stdin -- modelled on `run_editor` in `src/tui/infra/event_loop.rs`
- Zero defaults: when `[agents] interactive` is unset, interactive templates are not offered and cannot run
- Interactive runs are foreground, synchronous, and leave no `AgentRecord`
- Ignoring `allowed_tools` for interactive runs (the configured command owns tool policy)

### Out of Scope

- The `AgentRunner` trait, `AgentContext` / `AgentHandle`, and the headless `ClaudeP` / `claude -p` impl (Story 1)
- Template discovery, frontmatter parsing, minijinja rendering, and `child_types` context (Story 2) -- this story consumes the rendered body and the parsed `mode`
- The per-type `[[types]].agents` action allowlist gating (Story 3); that is the action allowlist, whereas the global `[agents] interactive` in this story is the launch command -- they stay separate
- The base agent dialog, headless dispatch, and freeform Custom prompt entry (Story 4); this story only adds the interactive dispatch branch on top
- A configurable headless command (headless stays `claude -p`)
