---
title: Context-aware status components
type: story
status: draft
author: jkaloger
date: 2026-03-13
tags: []
related:
- implements: docs/rfcs/RFC-021-lualine-inspired-status-bar.md
---


## Context

STORY-063 delivers the status bar layout and basic components. This story adds
the components that need external data (git branch) or document-graph awareness
(parent/child breadcrumb), and migrates the Agents screen's bespoke footer to
use the shared status bar.

## Acceptance Criteria

- **Given** the TUI is open in a git repository
  **When** the status bar renders
  **Then** the current git branch name appears in the left section after the panel name

- **Given** a document is selected that has a parent relationship (e.g. a Story implementing an RFC)
  **When** the status bar renders
  **Then** the center section shows a breadcrumb like "RFC-006 > STORY-042"

- **Given** a document is selected with no parent/child relationships
  **When** the status bar renders
  **Then** the center section is empty

- **Given** the user is on the Agents screen
  **When** the status bar renders
  **Then** the Agents-specific keybinding hints (e.g. "e: open  r: resume") appear in the right section instead of the generic help hint

- **Given** the old Agents footer implementation
  **When** this story is complete
  **Then** the bespoke Agents footer code is removed and replaced by status bar components

## Scope

### In Scope

- Git branch component (cached on TUI startup via `git rev-parse --abbrev-ref HEAD`)
- Parent/child breadcrumb component using selected doc's `related` field
- Agents footer migration to status bar
- Context-sensitive right section (Agents keybindings vs generic help hint)

### Out of Scope

- Live git branch polling (branch is read once at startup)
- Deep relationship chains (only immediate parent shown)
- Configurable component ordering or user-defined components
