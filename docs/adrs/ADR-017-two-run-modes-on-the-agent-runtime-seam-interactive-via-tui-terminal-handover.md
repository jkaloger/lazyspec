---
title: Interactive run mode via a configurable shell command and TUI terminal handover
type: adr
status: draft
author: jkaloger
date: 2026-06-18
tags: []
related:
- related-to: RFC-046
---

## Context

Interactive agent mode (RFC-046, RFC-016) runs agents headless: `claude -p` in the background, stdio discarded, polled for completion. A second need surfaced: invoking a live, interactive agent session on a document, like the TUI `e` command hands the terminal to `$EDITOR`, but for a live agent. A live session and a background job are different lifecycles -- one blocks and owns the terminal, the other returns a handle to poll. And the interactive tool varies by machine and project (`claude`, `opencode`, `pi`, or a custom shell/tmux invocation), so it cannot be hardcoded.

## Decision

A template declares one of two run modes via a `mode` frontmatter field (default `headless`); the TUI dialog dispatches by it. The two modes are built differently and are not two methods on one trait.

Headless stays behind `AgentRunner::spawn` (`ClaudeP`, `claude -p`), returning an `AgentHandle` to poll. Interactive is not a method on that trait: it has a single behaviour -- run whatever the project configured -- so per dictum 6 it earns no trait, and folding it into `AgentRunner` would force the claude-specific `ClaudeP` to execute arbitrary opencode/tmux commands.

The interactive command is a global `[agents] interactive` string in toml, run via `bash -lc`. The engine exports the rendered template body as `$LAZYSPEC_PROMPT` and the document path as `$LAZYSPEC_DOC_PATH`; the command references them (`claude "$LAZYSPEC_PROMPT"`, `opencode -p "$LAZYSPEC_PROMPT"`, `tmux new-window claude "$LAZYSPEC_PROMPT"`). Passing the prompt by environment variable rather than interpolating it into the command string avoids shell-quoting rendered markdown. Zero defaults (ADR-015): there is no built-in command; when `[agents] interactive` is unset, interactive actions are unavailable and not offered. `allowed_tools` is a `claude -p` concept and does not apply to interactive, where the configured command owns its tool policy.

Terminal state stays in the TUI. The suspend/run/restore sequence -- leave the alternate screen, disable raw mode, run the child to exit, restore, drain buffered stdin -- lives in the TUI event loop, modelled on `run_editor` (`src/tui/infra/event_loop.rs`). The engine builds the `Command` (program, args, env); it never touches terminal modes (dictum 3: the engine assumes no terminal). Interactive runs are synchronous and leave no `AgentRecord`, matching `e`.

Rejected: `run_interactive` as a second `AgentRunner` method (the interactive command is config, not a runtime impl; forcing it onto the trait makes `ClaudeP` run non-claude tools). Rejected: an argv array instead of a shell string (a custom tmux wrapper and pipes need a shell; the env-var prompt sidesteps argv quoting). Rejected: a per-template or per-type command (the tool is a machine/project property, not per-document). Rejected: terminal handling in the engine (violates dictum 3). Rejected: a baked default command (violates zero-defaults, ADR-015).

## Consequences

- The interactive tool is swappable in toml with no code change: `claude`, `opencode`, `pi`, or a custom shell/tmux command.
- The global `[agents]` block exists solely for this launch command; action gating stays per type and tool scope stays per template (ADR-016).
- A fresh project has no interactive command; interactive actions appear only after `[agents] interactive` is set.
- The TUI gains one more suspend/resume call site beyond the editor; both share the `run_editor` pattern.

