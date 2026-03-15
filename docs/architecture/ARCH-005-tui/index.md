---
title: "TUI"
type: arch
status: accepted
author: jkaloger
date: 2026-03-15
tags: [architecture, tui]
related:
  - related-to: "docs/rfcs/RFC-005-tui-flat-navigation-model.md"
  - related-to: "docs/rfcs/RFC-006-tui-progressive-disclosure.md"
  - related-to: "docs/rfcs/RFC-011-tui-ux-refinements.md"
  - related-to: "docs/rfcs/RFC-018-tui-interaction-enhancements.md"
  - related-to: "docs/stories/STORY-003-tui-dashboard.md"
---

# TUI

The TUI module (`src/tui/`) provides an interactive terminal interface built on
Ratatui and Crossterm. It runs a multi-threaded event loop with live file watching,
async ref expansion, and optional agent integration.

The TUI evolved through several RFCs:

- [RFC-003: TUI Document Creation](../../rfcs/RFC-003-tui-document-creation.md)
- [RFC-004: TUI Document Deletion](../../rfcs/RFC-004-tui-document-deletion.md)
- [RFC-005: TUI Flat Navigation Model](../../rfcs/RFC-005-tui-flat-navigation-model.md)
- [RFC-006: TUI Progressive Disclosure](../../rfcs/RFC-006-tui-progressive-disclosure.md)
- [RFC-011: TUI UX Refinements](../../rfcs/RFC-011-tui-ux-refinements.md)
- [RFC-016: Init agents from TUI](../../rfcs/RFC-016-init-agents-from-tui.md)
- [RFC-018: TUI Interaction Enhancements](../../rfcs/RFC-018-tui-interaction-enhancements.md)

Children cover:
- **threading-model**: Threads, event types, input pausing
- **event-loop**: Main loop, file watching
- **app-state**: App struct, view modes, overlays, caching
- **rendering**: Layout, diagram pipeline
- **agent-integration**: Agent spawner, actions, session resume
