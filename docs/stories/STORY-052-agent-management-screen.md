---
title: Agent management screen
type: story
status: draft
author: jkaloger
date: 2026-03-08
tags: []
related:
- implements: docs/rfcs/RFC-016-init-agents-from-tui.md
---


## Context

RFC-016 introduces agent support in the TUI. Users need a dedicated screen to
track agents they've invoked, see their status, and review output. This is the
visibility layer; invocation and prompt management are handled separately.

## Acceptance Criteria

- **Given** the TUI is running
  **When** the user navigates to the agents screen
  **Then** a list of all current and past agents is displayed

- **Given** agents exist in the agent list
  **When** the user views the list
  **Then** each agent shows its status (running, complete, or failed)

- **Given** an agent is selected in the list
  **When** the user presses the open/view keybinding
  **Then** the agent's output is opened in $EDITOR (matching the existing editor pattern)

- **Given** no agents have been invoked
  **When** the user navigates to the agents screen
  **Then** an empty state message is shown

- **Given** an agent transitions from running to complete or failed
  **When** the user is viewing the agents screen
  **Then** the status updates without requiring manual refresh

## Scope

### In Scope

- New TUI screen/tab dedicated to agent management
- List view of all current and past agents
- Agent status display (running, complete, failed)
- Ability to open/view agent output using the $EDITOR pattern
- Navigation to the agent screen from the main TUI

### Out of Scope

- Invoking or spawning agents (covered by slice 1: agent invocation from TUI)
- Custom prompt file management (covered by slice 3: custom agent prompts)
- Action selection dialog
- Support for non-Claude agents
