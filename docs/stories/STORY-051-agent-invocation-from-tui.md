---
title: Agent invocation from TUI
type: story
status: draft
author: jkaloger
date: 2026-03-08
tags:
- tui
- agents
- ai
related:
- implements: docs/rfcs/RFC-016-init-agents-from-tui.md
---


## Context

Users currently have no way to invoke AI agents from within the TUI. To iterate on documents (expanding RFCs, generating child stories/iterations), they must leave the TUI and manually orchestrate agent calls. This story adds the initial agent invocation flow: a keybinding, an action selection dialog, and headless Claude execution, so users can kick off common agent tasks without breaking their browsing flow.

## Acceptance Criteria

### AC1: Open agent dialog

**Given** a document is selected in the DocList panel
**When** the user presses `a`
**Then** a centered action selection dialog appears listing the available agent actions

### AC2: Expand document action

**Given** the agent dialog is open
**When** the user selects "Expand document"
**Then** a headless Claude agent is spawned that fleshes out the selected document's content

### AC3: Create children action

**Given** the agent dialog is open for a document whose type has child types defined in config (e.g. RFC -> Story)
**When** the user selects "Create children"
**Then** a headless Claude agent is spawned that generates child documents according to the parent-child rules in the lazyspec config

### AC4: Create children unavailable for leaf types

**Given** the agent dialog is open for a document whose type has no child types in config
**When** the dialog renders
**Then** the "Create children" option is not listed

### AC5: Custom prompt action

**Given** the agent dialog is open
**When** the user selects "Custom prompt"
**Then** a text input appears where the user can type a freeform prompt, and on submit a headless Claude agent is spawned with that prompt and the selected document as context

### AC6: Agent spawns in background

**Given** the user has selected an agent action and confirmed
**When** the agent is spawned
**Then** the TUI remains responsive and the user can continue navigating documents

### AC7: Cancel dialog

**Given** the agent dialog is open
**When** the user presses `Esc`
**Then** the dialog closes and no agent is spawned

### AC8: No document selected

**Given** the DocList panel is focused but empty
**When** the user presses `a`
**Then** nothing happens

### AC9: Dialog blocks other input

**Given** the agent dialog is open
**When** the user presses any key not handled by the dialog
**Then** the keypress is ignored

### AC10: Claude-only execution

**Given** an agent action is triggered
**When** the agent process is created
**Then** the agent is a headless Claude instance (no other provider support required)

## Scope

### In Scope

- `a` keybinding on the DocList panel to open the agent dialog
- Action selection dialog with "Expand document", "Create children", and "Custom prompt" options
- Deriving available actions from config parent-child rules (hiding "Create children" for leaf types)
- Freeform text input for custom prompts
- Headless Claude agent spawning in the background
- Claude-only support

### Out of Scope

- Agent history/status tracking screen (covered by slice 2)
- Custom prompt file discovery from `.lazyspec/agents/` (covered by slice 3)
- Agent status indicators or progress display in the TUI
- Support for non-Claude agent providers
- Bulk agent invocation across multiple documents
