---
title: Sequence skill
type: story
status: draft
author: jkaloger
date: 2026-04-30
tags: []
related:
- implements: RFC-041
priority: should
---



## Summary

A `/sequence` skill that opens the TUI sequencing screen on a chosen scope so a human can run the sequencing pass after horizontal planning. The skill is the entry point teams reach for when they need to wire `blocks` edges, set priorities, and inspect the critical path interactively. It accepts no arguments, a scope id, or `--after <id>`, prompting the user when the desired scope is ambiguous, and hands back to the user when the TUI exits.

## Scope

### In Scope

- A new `/sequence` skill invokable with no args, with a scope id, or with `--after <id>`.
- Scope resolution behaviour: prompt when ambiguous, pass through when explicit.
- Delegation to the existing `lazyspec` CLI to launch the TUI sequencing screen.
- Returning control to the user after the TUI exits.

### Out of Scope

- The TUI sequencing screen itself (Stories 4a/4b/4c, prerequisite).
- The `/next-work` skill (Story 6).
- Horizontal planning skill `/plan-project` (separate RFC).
- CLI surfaces for `next`, `graph`, `critical-path` (Story 3).
- Engine graph primitives (Story 1).

## Acceptance Criteria

- **Given** the user invokes `/sequence` with no arguments
  **When** the skill runs
  **Then** the user is prompted to choose between whole-project scope, an `--scope <id>`, or `--after <id>` before the TUI is launched.

- **Given** the user invokes `/sequence <id>` where `<id>` is a document with implements-descendants
  **When** the skill runs
  **Then** the TUI sequencing screen opens scoped under `<id>` without further prompting.

- **Given** the user invokes `/sequence --after <id>`
  **When** the skill runs
  **Then** the TUI sequencing screen opens in after-mode for `<id>` without further prompting.

- **Given** the user invokes `/sequence <id>` where `<id>` is an iteration
  **When** the skill runs
  **Then** the invocation is rejected and the user is shown a hint matching the CLI's rejection message for iteration scopes.

- **Given** the TUI sequencing screen is open via the skill
  **When** the user exits the TUI
  **Then** control returns to the user in the conversation without the skill performing further actions.

- **Given** the user invokes `/sequence` with both a scope id and `--after <id>`
  **When** the skill runs
  **Then** the invocation is rejected with a message stating the two flags are mutually exclusive.
