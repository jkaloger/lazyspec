---
title: Status bar widget and default components
type: story
status: complete
author: agent
date: 2026-03-30
tags: []
related:
- implements: RFC-022
priority: should
---




## Context

The TUI currently provides no persistent status information. Users must run separate commands or mentally track state to know basic things like document counts, validation health, or which mode they're in. A status bar at the bottom of the terminal, similar to neovim's lualine, gives ambient awareness without demanding attention.

This story covers the foundational status bar widget and its default set of components. Later stories add more components, fullscreen behavior, and configuration.

## Acceptance Criteria

- Given the TUI is open,
  When the terminal renders,
  Then a status bar is visible in the bottom row of the terminal with a visually distinct background color.

- Given the TUI is open in any view mode,
  When the user looks at the status bar's left section,
  Then the current mode name is displayed.

- Given the TUI is open with documents loaded,
  When the user looks at the status bar's left section,
  Then the count of currently visible documents is displayed.

- Given the project has documents with validation warnings,
  When the user looks at the status bar's center section,
  Then the warning count is displayed in yellow text.

- Given the project has documents with validation errors,
  When the user looks at the status bar's center section,
  Then the error count is displayed in red text.

- Given the project has no validation warnings or errors,
  When the user looks at the status bar's center section,
  Then the corresponding warning or error indicator is not shown.

- Given the TUI is open,
  When the user looks at the status bar's center section,
  Then the application version is displayed.

- Given the TUI is open,
  When the user looks at the status bar's right section,
  Then a help hint is displayed indicating how to access help.

- Given the old standalone help hint from the content area exists,
  When the status bar is rendered,
  Then the standalone hint is removed from its previous location and only appears in the status bar.

- Given the status bar has components in the same zone,
  When multiple components produce output,
  Then they are separated by a visual delimiter.

- Given a component has nothing to display,
  When the status bar renders,
  Then that component is silently omitted without leaving blank space or extra separators.

## Scope

### In Scope

- Status bar widget occupying the bottom terminal row
- Three-zone layout: left-aligned, centered, right-aligned
- Component abstraction that reads app state and optionally produces styled text
- Built-in components: mode, doc_count, warnings, errors, version, help_hint
- Distinct background color and per-component foreground colors (yellow for warnings, red for errors)
- ANSI color usage for terminal compatibility
- Separator rendering between adjacent components in the same zone
- Removal of the standalone `? help` hint, absorbed into the status bar

### Out of Scope

- `git_branch`, `search`, and `type_filter` components (Story 2)
- Fullscreen hide behavior (Story 2)
- Configuration via `.lazyspec.toml` (Story 3)
- Component reordering or user-defined components (Story 3)
