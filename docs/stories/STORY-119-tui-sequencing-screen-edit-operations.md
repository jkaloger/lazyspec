---
title: TUI sequencing screen edit operations
type: story
status: draft
author: jkaloger
date: 2026-04-30
tags: []
related:
- implements: RFC-041
- blocks: STORY-114
priority: should
---




## Context

The new TUI sequencing screen (Story 4a) renders the `blocks` DAG with scope filtering. To make sequencing a live planning surface rather than a viewer, users need to mutate the DAG directly: add edges, remove edges, and recover from mistakes. RFC-041's TUI section calls out three edit-related bullets: adding a `blocks` edge with cycle prevention, removing a selected edge with delete, and ensuring edits are atomic via the engine link/unlink path so on-disk markdown stays the source of truth.

This slice covers exactly those edit operations and a session-bounded undo. Cycle detection itself lives in the engine (Story 1); this slice consumes it and surfaces rejections in the status bar. Priority editing, the budget panel, and the critical-path overlay are deferred to Story 4c.

## Acceptance Criteria

- **Given** the sequencing screen is open with two documents A and B and no edge between them,
  **When** the user selects A as source, then B as target, and confirms an add-edge action,
  **Then** a `blocks` edge from A to B is rendered, A's source markdown frontmatter on disk now lists B as a `blocks` target, and no other documents are modified.

- **Given** the sequencing screen is open and the user is mid-add with source A selected,
  **When** the user picks a target B such that A blocks B would induce a cycle,
  **Then** no edge is rendered, no markdown file on disk is modified, and the status bar shows an error message identifying the cycle and the rejected edge.

- **Given** the sequencing screen is open and an existing `blocks` edge from A to B is selected,
  **When** the user presses delete and confirms,
  **Then** the edge is removed from the rendered DAG and A's source markdown no longer lists B in its `blocks` field.

- **Given** the user has just added a `blocks` edge from A to B in the current session,
  **When** the user invokes undo,
  **Then** the edge is removed from the DAG, A's source markdown is restored to its pre-add state, and a subsequent redo or re-inspection shows the file matches what was on disk before the add.

- **Given** the user has just removed a `blocks` edge from A to B in the current session,
  **When** the user invokes undo,
  **Then** the edge reappears in the DAG and A's source markdown again lists B as a `blocks` target.

- **Given** the user performs a sequence of N edit operations in a session,
  **When** the user invokes undo more than N times,
  **Then** undo unwinds at most N operations, no further file changes occur once the session-op stack is empty, and the status bar indicates nothing more can be undone.

- **Given** the user closes and reopens the sequencing screen,
  **When** the user invokes undo,
  **Then** no edits from prior sessions are reversed; the undo stack is bounded to the current session's operations.

- **Given** any add or remove edit is invoked,
  **When** the engine's link or unlink call fails,
  **Then** no partial state is written: the rendered DAG matches the on-disk markdown after the failed operation and the status bar surfaces the failure.

## Scope

### In Scope

- Add-edge interaction: selecting source then target on the sequencing screen, persisting via the engine link path.
- Cycle rejection: surfacing the engine's cycle-induction error in the status bar with no file write.
- Remove-edge interaction: selecting an existing `blocks` edge and persisting the unlink via the engine.
- Session-scoped undo for the last N add and remove operations.
- Atomicity: each edit either fully persists through the engine or leaves both the DAG and on-disk markdown unchanged.

### Out of Scope

- Sequencing screen render and scope filter (Story 4a, prerequisite).
- Priority editing via numeric keys, budget panel, critical-path overlay (Story 4c).
- Engine-side cycle detection and the link/unlink primitives themselves (Story 1).
- CLI sequencing commands (Story 3).
- `/sequence` and `/next-work` skills (Stories 5 and 6).
- Cross-session undo or persisted edit history.
