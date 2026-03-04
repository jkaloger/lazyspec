---
title: Flat navigation over panel focus
type: adr
status: accepted
author: jkaloger
date: 2026-03-05
tags: []
related:
- related-to: docs/rfcs/RFC-005-tui-flat-navigation-model.md
---



## Decision

Use a flat navigation model where `h/l` cycles document types and `j/k` navigates documents, rather than the current panel-focus model where `h/l` switches between independently navigable panels.

## Context

The original TUI followed a common pattern for multi-panel layouts: each panel is a focusable surface, and directional keys switch focus between them. This works well when panels contain different kinds of content that users interact with equally. In lazyspec, the Types panel contains four static items and the DocList is where all real interaction happens. The panel-focus model adds a layer of indirection (tracking which panel is active) without proportional benefit.

The Relations tab compounds this by hijacking `j/k` when active, creating context-dependent keybindings that are hard to predict.

## Alternatives Considered

**Keep panel focus, simplify relations.** We could have kept the panel model and only removed the relation navigation hijack. This would reduce the worst surprise but still leave users managing panel focus for a four-item list that rarely changes.

**Tab bar instead of sidebar.** Replace the Types sidebar with a horizontal tab bar. This would reclaim horizontal space but changes the layout significantly. The sidebar works fine visually; the issue is the interaction model, not the layout.

## Consequences

- The `Panel` enum and `active_panel` field are removed. Any code that branches on active panel needs updating.
- `selected_relation` is removed. The relations tab becomes view-only.
- Border highlighting becomes static rather than reactive, since there is only one navigable surface.
- `d` for delete no longer gates on `active_panel == DocList`. It gates on whether a document is selected.
- Future features that add new navigable panels would need a different approach (modal overlays, new keybindings) rather than extending the panel focus model.
