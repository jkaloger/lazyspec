---
title: TUI Status Bar
type: rfc
status: draft
author: jkaloger
date: 2026-03-15
tags:
- tui
- ux
- status-bar
related:
- related to: RFC-011
- related to: RFC-018
---


## Problem

The TUI has no persistent status information. Users can't see at a glance how many documents exist, whether validation is passing, or which filters are active. The only contextual information is the mode strip in the title bar (`[1]Types [2]Filters ...`) and whatever happens to be visible in the current view.

This means users have to:
- Run `lazyspec validate` separately to check project health
- Count documents manually or switch to Metrics mode
- Remember which type they're looking at when the type list scrolls off-screen

Terminal tools like neovim (via lualine/lightline), lazygit, and btop all solve this with a status bar that provides ambient awareness without demanding attention.

## Intent

Add a composable status bar to the bottom of the TUI, inspired by neovim's lualine. The bar is divided into sections (left, center, right) with configurable components. It occupies a single terminal row and updates reactively as state changes.

The status bar should feel like part of the TUI's existing visual language, not a bolt-on. It replaces the current `? help` hint from RFC-011 by absorbing it into the right section.

## Design

### Layout

The status bar sits in the last row of the terminal, below all other content. It's a single-row widget with three alignment zones:

```
┌────────────────────────────────────────────────────────────────┐
│  ● Types  │  12 docs  3 ⚠  0 ✗       lazyspec v0.4      ? help │
│  [left]                  [center]                      [right] │
└────────────────────────────────────────────────────────────────┘
```

Each zone is a list of **components**. Components are small functions that read `App` state and return a `Span` (or nothing, if the component has nothing to show).

### Built-in Components

| Component | Zone | Output | Source |
|-----------|------|--------|--------|
| `mode` | left | Current `ViewMode` name with icon | `app.view_mode` |
| `type_filter` | left | Active type name when in Types mode | `app.selected_type` |
| `doc_count` | left | Count of visible documents | `app.doc_tree.len()` |
| `warnings` | center | Warning count from validation | `app.store.validate()` |
| `errors` | center | Error count from validation | `app.store.validate()` |
| `version` | center | `lazyspec v{VERSION}` | compile-time const |
| `help_hint` | right | `? help` | static |
| `search` | right | Active search query when searching | `app.search_query` |
| `git_branch` | right | Current git branch name | `git rev-parse` at startup |

Components that produce empty output are silently omitted (no blank separators).

### Rendering

@ref src/tui/ui.rs#draw

The `draw` function currently splits the terminal into mode-specific areas. The status bar adds one constraint: reserve the bottom row before layout calculation.

```rust
@draft StatusBar {
    left: Vec<StatusComponent>,
    center: Vec<StatusComponent>,
    right: Vec<StatusComponent>,
}

@draft StatusComponent {
    // Each component is a function: &App -> Option<Span>
    // Returns None when the component has nothing to display
}
```

The bar renders by:
1. Evaluating each component in each zone
2. Filtering out `None` results
3. Joining spans with a separator (` │ ` for adjacent components in the same zone)
4. Left-aligning the left zone, centering the center zone, right-aligning the right zone
5. Drawing as a single `Paragraph` widget with styled background

### Styling

The status bar uses a distinct background color to visually separate it from content. Default: `Color::DarkGray` background, `Color::White` foreground. Individual components can override foreground color (e.g. warnings in yellow, errors in red).

### Configuration

The status bar is configurable via `.lazyspec.toml`:

```toml
[tui.statusbar]
enabled = true                              # default: true
left = ["mode", "type_filter", "doc_count"] # default
center = ["warnings", "errors"]             # default
right = ["version", "help_hint"]            # default
```

Omitting the `[tui.statusbar]` section entirely uses the defaults above. Setting `enabled = false` hides the bar and reclaims the row for content.

Invalid component names in config are silently ignored with a validation warning (consistent with how lazyspec handles other config issues).

@ref src/engine/config.rs#Config

### Git Branch

The git branch is read once at startup via `git rev-parse --abbrev-ref HEAD`. This is a blocking call but only runs once. If the command fails (not a git repo, git not installed), the `git_branch` component produces `None` and is omitted.

The branch is not refreshed during the session. This matches lazygit's behavior and avoids spawning processes on every render tick.

### Interaction with Existing UI

The `? help` hint currently renders in the bottom-left corner (RFC-011). With the status bar, the hint moves into the `help_hint` component in the right zone. The standalone hint rendering is removed.

The mode strip in the title bar remains unchanged. The status bar's `mode` component shows the current mode name with its icon, providing redundant-but-useful context, especially when the title bar is partially obscured by overlays.

Fullscreen mode (`Enter` on a document) hides the status bar to maximize preview space. Modal overlays render on top of the status bar.

## Stories

1. **Status bar widget and default components** -- `StatusBar` struct, built-in components (`mode`, `doc_count`, `warnings`, `errors`, `version`, `help_hint`), rendering in `draw()`, bottom-row layout reservation. Absorb `? help` hint.

2. **Git branch and search components** -- `git_branch` component (read at startup), `search` component (shows active query), `type_filter` component.

3. **Status bar configuration** -- `[tui.statusbar]` section in `.lazyspec.toml`, component ordering, `enabled` toggle. Config parsing and validation.
