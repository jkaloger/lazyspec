---
title: Status bar configuration
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

RFC-022 introduces a composable status bar for the TUI. This story covers the configuration layer: parsing the `[tui.statusbar]` section from `.lazyspec.toml`, applying defaults when the section is omitted, toggling visibility via `enabled`, and validating component names.

## Acceptance Criteria

- **Given** no `[tui.statusbar]` section exists in `.lazyspec.toml`
  **When** the application loads configuration
  **Then** the status bar is enabled with default components: left = `["mode", "type_filter", "doc_count"]`, center = `["warnings", "errors"]`, right = `["version", "help_hint"]`

- **Given** a `[tui.statusbar]` section with `enabled = false`
  **When** the application loads configuration
  **Then** the status bar is hidden and its row is reclaimed for content

- **Given** a `[tui.statusbar]` section with `enabled = true` (or `enabled` omitted)
  **When** the application loads configuration
  **Then** the status bar is visible

- **Given** a `[tui.statusbar]` section with custom `left`, `center`, or `right` arrays
  **When** the application loads configuration
  **Then** the configured components appear in the specified zones in the order listed

- **Given** a `[tui.statusbar]` section that defines only some zones (e.g. only `left`)
  **When** the application loads configuration
  **Then** the unspecified zones use their default component lists

- **Given** a `[tui.statusbar]` section containing an invalid component name
  **When** the application loads configuration
  **Then** the invalid component is silently ignored and a validation warning is emitted

- **Given** a `[tui.statusbar]` section with an empty array for a zone (e.g. `left = []`)
  **When** the application loads configuration
  **Then** that zone renders with no components

## Scope

### In Scope

- `[tui.statusbar]` section in `.lazyspec.toml`
- `enabled` toggle (default: true); setting to false hides the bar and reclaims its row
- Component ordering per zone via `left`, `center`, `right` arrays
- Sensible defaults when `[tui.statusbar]` is omitted entirely
- Invalid component names silently ignored with a validation warning
- Config parsing and validation logic

### Out of Scope

- StatusBar widget, zone layout, and rendering (Story 1)
- Individual component implementations (Stories 1 and 2)
- Styling customization beyond what the status bar already provides
