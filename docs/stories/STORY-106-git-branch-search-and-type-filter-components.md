---
title: Git branch, search, and type filter components
type: story
status: accepted
author: agent
date: 2026-03-30
tags: []
related:
- implements: RFC-022
priority: should
---





## Context

RFC-022 introduces a composable status bar for the TUI. This story covers the secondary components that depend on runtime state: git branch detection, search query display, and type filter display. It also covers the interaction between the status bar and fullscreen/modal modes.

## Acceptance Criteria

- **Given** the TUI is launched inside a git repository
  **When** the status bar renders
  **Then** the current git branch name is displayed in the status bar

- **Given** the TUI is launched outside a git repository
  **When** the status bar renders
  **Then** no git branch information appears in the status bar

- **Given** git is not installed on the system
  **When** the TUI is launched
  **Then** the status bar renders without a git branch component and no error is shown

- **Given** the TUI is running and the status bar is visible
  **When** the user initiates a search and types a query
  **Then** the active search query is displayed in the status bar

- **Given** the user has an active search query displayed in the status bar
  **When** the user exits search mode
  **Then** the search query is no longer displayed in the status bar

- **Given** the TUI is displaying documents of all types
  **When** the user activates a type filter (e.g. filtering to stories only)
  **Then** the active type name is displayed in the status bar

- **Given** a type filter is active and shown in the status bar
  **When** the user clears the type filter
  **Then** the type name is no longer displayed in the status bar

- **Given** the status bar is visible with components rendered
  **When** the user enters fullscreen mode on a document
  **Then** the status bar is hidden to maximize preview space

- **Given** the user is in fullscreen mode with the status bar hidden
  **When** the user exits fullscreen mode
  **Then** the status bar reappears with all components intact

- **Given** the status bar is visible
  **When** a modal overlay is triggered
  **Then** the modal renders on top of the status bar

## Scope

### In Scope

- `git_branch` component that reads the branch name once at startup and produces nothing if git is unavailable or the directory is not a repo
- `search` component that reactively displays the current search query during search mode
- `type_filter` component that displays the active type name when a type filter is engaged
- Fullscreen mode hides the status bar
- Modal overlays render on top of the status bar

### Out of Scope

- StatusBar struct, zone layout, and rendering logic (Story 1)
- Core components: mode, doc_count, warnings, errors, version, help_hint (Story 1)
- Configuration via .lazyspec.toml (Story 3)
- Refreshing the git branch during the session
