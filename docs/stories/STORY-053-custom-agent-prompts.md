---
title: Custom agent prompts
type: story
status: draft
author: jkaloger
date: 2026-03-08
tags: []
related:
- implements: docs/rfcs/RFC-016-init-agents-from-tui.md
---


## Context

RFC-016 introduces agent invocation from the TUI. Users need a way to define custom prompts that agents can use when operating on documents. These prompts live in `.lazyspec/agents/` and are discovered at runtime, allowing project-specific agent behaviours without modifying lazyspec itself.

## Acceptance Criteria

### AC1: Directory convention

**Given** a lazyspec project
**When** the `.lazyspec/agents/` directory exists
**Then** it is recognised as the location for custom agent prompt files

### AC2: Prompt file format

**Given** a file in `.lazyspec/agents/`
**When** the file is read
**Then** it contains YAML frontmatter with `name` and `description` fields, followed by the prompt body as markdown

### AC3: Prompt discovery

**Given** one or more prompt files exist in `.lazyspec/agents/`
**When** the agent invoke dialog is opened
**Then** all valid prompt files are discovered and available for selection

### AC4: Prompt listing

**Given** prompt files exist in `.lazyspec/agents/`
**When** the user views the prompt selection list
**Then** each prompt displays its `name` and `description` from frontmatter

### AC5: Invalid prompt handling

**Given** a file in `.lazyspec/agents/` with missing or malformed frontmatter
**When** prompt discovery runs
**Then** the file is skipped with a warning and does not appear in the selection list

### AC6: Template variable interpolation

**Given** a prompt body containing template variables (e.g. `{{document.title}}`, `{{document.type}}`, `{{document.body}}`)
**When** the prompt is selected for a specific document
**Then** the variables are replaced with values from the target document before being passed to the agent

### AC7: No prompts available

**Given** the `.lazyspec/agents/` directory is empty or does not exist
**When** the agent invoke dialog shows the custom prompt option
**Then** the option is disabled or hidden, with a message indicating no custom prompts are configured

## Scope

### In Scope

- `.lazyspec/agents/` directory convention
- Prompt file format: YAML frontmatter (`name`, `description`) + markdown body
- Discovery and listing of prompt files
- Prompt selection within the agent invoke dialog
- Template variable interpolation using document context
- Graceful handling of missing/invalid prompt files

### Out of Scope

- Agent execution and runtime
- Agent status tracking or history
- The `a` keybinding and invoke dialog itself (covered by STORY-051)
- Built-in actions (expand, create children)
- Agent management screen (covered by STORY-052)
- Support for non-Claude agents
