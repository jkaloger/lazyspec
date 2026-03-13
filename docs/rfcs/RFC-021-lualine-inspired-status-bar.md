---
title: "lualine inspired status bar"
type: rfc
status: draft
author: "jkaloger"
date: 2026-03-13
tags: [tui, ux]
---

## Summary

Inspired by [lualine.nvim](https://github.com/nvim-lualine/lualine.nvim), add a
persistent status bar (footer) to the lazyspec TUI. The bar provides at-a-glance
context without requiring the user to open overlays or remember which screen
they're on.

## Motivation

The current TUI has limited persistent context. The header shows a title and
mode hint, but there's no git branch indicator, no document relationship
breadcrumb, and help discoverability relies on the `?` overlay. Only the Agents
screen has a footer, and it's a one-off implementation.

A unified status bar solves all of these and creates a consistent foundation
that new components can plug into.

## Design

### Layout: 3-section footer

A single-line footer rendered at the bottom of every screen, divided into three
sections:

```
 Left              Center                          Right
┌──────────────┬──────────────────────┬──────────────────┐
│ Types │ main │   RFC-006 > STORY-042│       ? for help │
└──────────────┴──────────────────────┴──────────────────┘
```

| Section | Alignment | Content |
|---------|-----------|---------|
| Left    | Left      | Current panel/mode, git branch |
| Center  | Center    | Parent/child relationship breadcrumb for selected doc |
| Right   | Right     | Help hint, contextual info |

Sections are separated by a simple `│` character. No powerline glyphs to keep
terminal compatibility broad.

### Theming

Static colors, consistent across all ViewModes. The current panel label in the
left section provides sufficient mode indication without needing the whole bar
to change color.

Palette follows existing conventions: Cyan for emphasis, DarkGray for
separators, White for content.

### Components

Each piece of status information is a discrete component that returns a
`ratatui::text::Span` (or `Line`). Components are composed into sections.

| Component | Section | Source | Notes |
|-----------|---------|--------|-------|
| Panel indicator | Left | `app.view_mode` | e.g. "Types", "Filters", "Agents" |
| Git branch | Left | `git rev-parse --abbrev-ref HEAD` | Cached on startup, not live-polled |
| Doc breadcrumb | Center | Selected doc's `related` field | Shows parent > child chain |
| Help hint | Right | Static | "? for help" |

### Integration with existing UI

- The Agents screen's bespoke footer gets replaced by the status bar, with its
  keybinding hints moved into the status bar's right section contextually.
- The header's mode indicator (`[Types] backtick to cycle`) moves into the status bar's
  left section. The header retains just the "lazyspec" title.
- All ViewModes (Types, Filters, Metrics, Graph, Agents) get the footer.

## Stories

1. **Status bar foundation** -- Infrastructure + basic components (panel indicator, help hint).
   Adds the footer layout to all screens and the 3-section rendering model.

2. **Context-aware status components** -- Git branch, parent/child breadcrumb,
   Agents footer migration. Builds on the foundation with components that
   require external data or cross-screen consistency.
