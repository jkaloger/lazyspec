---
title: Styled CLI Output
type: story
status: accepted
author: jkaloger
date: 2026-03-05
tags:
- cli
- styling
related:
- implements: docs/rfcs/RFC-007-agent-native-cli.md
---



## Context

The CLI's human-readable output is plain unformatted text. Every command uses raw `println!` with fixed-width format strings. Titles, statuses, and file paths all render at the same visual weight, making output hard to scan. Charmbracelet's CLI tools (gum, gh, charm) demonstrate that terminal output can have clear visual hierarchy without sacrificing information density.

## Acceptance Criteria

- **Given** a terminal that supports ANSI colors
  **When** any human-readable CLI command runs (`list`, `status`, `show`, `validate`, `context`, `search`)
  **Then** output uses colored status badges (green/accepted, yellow/draft, red/rejected, grey/superseded) and dimmed file paths

- **Given** the `status` command runs without `--json`
  **When** documents are grouped by type
  **Then** each group has a styled section header with a border, and documents are displayed in an aligned table within each section

- **Given** the `show` command runs without `--json`
  **When** a document is displayed
  **Then** the title renders in a bordered header box, metadata is styled with labels dimmed and values highlighted, and the body renders below a separator

- **Given** the `validate` command runs and finds issues
  **When** errors or warnings are displayed
  **Then** errors use a red prefix/icon and warnings use a yellow prefix/icon

- **Given** the `context` command runs without `--json`
  **When** the chain is displayed
  **Then** each document in the chain is rendered as a styled card with a visible connector between them

- **Given** a terminal that does not support ANSI colors (piped output, dumb terminal)
  **When** any CLI command runs
  **Then** output falls back to plain unformatted text with no escape sequences

## Scope

### In Scope

- Colored status badges across all commands
- Styled section headers and borders for `status` and `show`
- Styled error/warning output for `validate`
- Chain visualization for `context`
- Automatic detection of terminal color support with plain-text fallback
- Consistent styling primitives shared across commands

### Out of Scope

- Interactive prompts or spinners
- TUI changes (ratatui is separate)
- JSON output formatting (already handled)
- New CLI commands or flags (beyond what's needed for color control)
