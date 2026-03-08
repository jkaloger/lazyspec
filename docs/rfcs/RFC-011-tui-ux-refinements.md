---
title: TUI UX Refinements
type: rfc
status: accepted
author: jkaloger
date: 2026-03-06
tags:
- tui
- ux
- keybindings
related:
- related-to: docs/rfcs/RFC-006-tui-progressive-disclosure.md
---



## Problem

The TUI has three interaction gaps that hurt usability:

1. Documents can't be edited from within the TUI. Users have to find the file path, switch to another terminal, and open it manually. For a tool that's meant to be the primary interface for managing specs, this friction discourages use.

2. Keybindings are invisible. The help overlay (`?`) exists but nothing on screen tells the user it's there. New users stare at the interface with no idea what keys do what. The help overlay is also static -- it shows the same keybindings regardless of which mode you're in, including keys that aren't relevant.

3. Mode switching uses backtick, which is obscure and requires cycling through modes sequentially. With four modes, getting from Types to Graph means pressing backtick three times. The title bar says `` ` to cycle `` which tells you *how* but not *what* -- you can't see which modes exist or where you are in the cycle.

## Intent

Refine the TUI's interaction model with three targeted changes that make the tool more usable without adding complexity. Each change is small and independently shippable.

## 1. Open in `$EDITOR`

Pressing `e` on a selected document suspends the TUI and opens the document in the user's preferred editor. When the editor exits, the TUI resumes and reloads the document.

This follows the terminal suspend/resume pattern used by lazygit, tig, and similar tools:

1. Leave alternate screen (`LeaveAlternateScreen`)
2. Disable raw mode
3. Spawn `$EDITOR <path>` as a foreground child process, wait for exit
4. Re-enable raw mode
5. Re-enter alternate screen (`EnterAlternateScreen`)
6. Reload the edited document from disk

Editor resolution follows the standard fallback chain: `$EDITOR` -> `$VISUAL` -> `vi`.

The `e` key works anywhere a document is selected -- Types mode, Filters mode, and Graph mode (on the selected node). It does not work during modal states (create form, delete confirm, search).

```
@ref src/tui/app.rs#App
@ref src/tui/app.rs#handle_key_event
```

> [!NOTE]
> crossterm provides `LeaveAlternateScreen` and `EnterAlternateScreen` commands. The suspend/resume sequence should be extracted into a helper that any future "shell out" feature can reuse.

## 2. Mode-aware help overlay with `?` hint

Two changes to help discoverability:

**Persistent hint:** A small `? help` label renders in the bottom-left corner of every screen, every mode. It costs one line of height and a few characters of width. It's styled dimly (DarkGray) so it doesn't compete with content.

```
┌─ Types ─────────────┐ ╔═ Documents ═══════════════════════════════╗
│  RFCs        (5)    │ ║  ...                                      ║
│  ...                │ ╚═══════════════════════════════════════════╝
└─────────────────────┘ ┌─ Preview ─────────────────────────────────┐
                        │  ...                                      │
                        └───────────────────────────────────────────┘
? help
```

**Mode-aware overlay:** The `?` overlay becomes context-sensitive. It shows keybindings relevant to the current `ViewMode`, grouped by section. In Graph mode, it additionally includes the icon and edge type legend.

Types/Filters mode help:

```
 Keybindings (Types)

  Navigation
  h/l       Switch type
  j/k       Navigate documents
  Enter     Open fullscreen
  g/G       Jump to top/bottom

  Actions
  e         Open in $EDITOR
  n         Create document
  d         Delete document
  /         Search
  Tab       Switch preview tab

  Modes
  1-4       Switch mode

  ?         Close help
```

Graph mode help (includes visual legend):

```
 Keybindings (Graph)

  Navigation
  j/k       Navigate nodes
  Enter     Jump to document
  g/G       Jump to top/bottom

  Actions
  e         Open in $EDITOR

  Modes
  1-4       Switch mode

  Legend
  ●  RFC       ■  ADR
  ▲  Story     ◆  Iteration

  Edges
  ├─▶  implements

  ?         Close help
```

```
@ref src/tui/ui.rs#draw_help_overlay
@ref src/tui/app.rs#ViewMode
```

The `draw_help_overlay` function takes `&App` (currently takes no arguments) so it can read `app.view_mode` and render the appropriate content.

## 3. Direct mode switching with number keys

Replace backtick cycling with direct number key access:

| Key | Mode |
|-----|------|
| `1` | Types |
| `2` | Filters |
| `3` | Metrics |
| `4` | Graph |

The title bar changes from `` [Types] ` to cycle `` to a mode strip showing all modes, with the active one highlighted:

```
  lazyspec                        [1]Types  [2]Filters  [3]Metrics  [4]Graph
```

The active mode renders in cyan/bold. Inactive modes render in DarkGray. This gives immediate visibility into what modes exist, how to reach them, and where you currently are.

```
@ref src/tui/app.rs#cycle_mode
@ref src/tui/ui.rs (line 88-95, mode indicator rendering)
```

The `cycle_mode` method on `App` is replaced with `set_mode(ViewMode)`. The `ViewMode::next()` method can stay for internal use but is no longer bound to a key.

The backtick binding is removed entirely. Number keys `1-4` are handled in the normal key dispatch (not during modal states).

## Stories

1. **Open in editor** -- `e` key handler, terminal suspend/resume, `$EDITOR`/`$VISUAL`/`vi` fallback chain, document reload on editor exit. Works in Types, Filters, and Graph modes.

2. **Mode-aware help and `?` hint** -- Persistent `? help` label in bottom-left corner. Refactor `draw_help_overlay` to accept `&App` and render mode-specific keybinding sections. Add graph icon/edge legend to Graph mode help.

3. **Direct mode switching** -- Replace backtick cycling with `1-4` number keys. Update title bar to show mode strip with all modes labeled. Remove `cycle_mode`, add `set_mode(ViewMode)`.
