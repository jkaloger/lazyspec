---
title: Template-driven TUI agent action dialog
type: story
status: draft
author: jkaloger
date: 2026-06-18
tags: []
related:
- implements: RFC-046
- supersedes: STORY-051
---

## Context

The TUI agent dialog (`a` on a selected document) currently offers a fixed set of built-in actions: Expand document, Create children, and Custom prompt. RFC-046 makes agent mode unopinionated, so the dialog must instead list the prompt templates resolved for the selected document's type -- each shown by its frontmatter `name` and `description` -- alongside one freeform Custom prompt entry. This story rewires the dialog to be template-driven and supersedes STORY-051's fixed action set; selecting a headless template spawns it in the background through the agent runner with the rendered prompt and the template's `allowed_tools`.

## Acceptance Criteria

### AC1: Dialog lists resolved templates by name and description

**Given** a document is selected whose type resolves to one or more prompt templates
**When** the user presses `a`
**Then** the action dialog opens listing one entry per resolved template, each labelled with the template's frontmatter `name` and `description`

### AC2: Custom prompt entry is always offered when agents are available

**Given** the agent dialog is open for a document whose type exposes agents
**When** the dialog renders
**Then** a freeform "Custom prompt" entry is present alongside any resolved template entries

### AC3: No built-in Expand or Create-children entries

**Given** the agent dialog is open for any document
**When** the dialog renders
**Then** no built-in "Expand document" or "Create children" entries appear -- the only entries are resolved templates and the Custom prompt entry

### AC4: Selecting a headless template spawns via the runner

**Given** the agent dialog is open with a resolved headless template entry selected
**When** the user confirms the selection
**Then** the agent runner is invoked with the template's rendered prompt and its `allowed_tools`, an agent record is created, and the dialog closes

### AC5: TUI stays responsive after a headless spawn

**Given** the user has confirmed a headless template action
**When** the agent has been spawned in the background
**Then** control returns immediately and the user can continue navigating documents while the agent runs

### AC6: Custom prompt captures freeform text and spawns with runtime defaults

**Given** the agent dialog is open
**When** the user selects "Custom prompt", types a prompt, and submits
**Then** a headless agent is spawned via the runner with that text as the prompt, the selected document as context, and no `allowed_tools` restriction beyond the runtime default

### AC7: Empty resolved set shows only Custom (or nothing)

**Given** a document whose type resolves to no templates
**When** the user presses `a`
**Then** the dialog offers only the Custom prompt entry, and offers nothing (no dialog, or an empty dialog) when the type exposes no agents and the project authors no Custom path

### AC8: Esc cancels the dialog

**Given** the agent dialog is open
**When** the user presses `Esc`
**Then** the dialog closes and no agent is spawned

## Scope

### In Scope

- Rewiring the `a`-keybinding action dialog to build its entry list from the templates resolved for the selected document's type (consuming the resolved action set from Story 3)
- Rendering each template entry by its frontmatter `name` and `description`
- One freeform "Custom prompt" entry: text input, no template, no `allowed_tools` restriction beyond the runtime default
- Dispatching a selected headless template through the agent runner with the rendered prompt and the template's `allowed_tools`, recording the run via the existing background-record flow
- Keeping the TUI responsive after a background spawn (return immediately)
- Handling the empty-resolved-set case (Custom only, or nothing)
- Esc to cancel with no spawn

### Out of Scope

- The `AgentRunner` trait and `ClaudeP` internals (Story 1) -- this story consumes the runner
- Template loading, frontmatter parsing, minijinja rendering, and `child_types` context (Story 2) -- this story consumes resolved and rendered prompts
- The per-type `[[types]].agents` resolution logic (Story 3) -- this story consumes the resolved action set
- INTERACTIVE run mode: terminal handover, the suspend/run/restore sequence, the `[agents] interactive` command, and any `mode: interactive` dispatch (Story 5) -- this story handles the dialog plus headless dispatch and freeform Custom only
- Authoring or shipping any prompt templates; the engine ships none
- The agent management/status screen (STORY-052) and the run-history directory relocation
