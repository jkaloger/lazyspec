---
title: "Rendering"
type: arch
status: accepted
author: jkaloger
date: 2026-03-15
tags: [architecture, tui, rendering]
related:
  - related-to: "docs/stories/STORY-057-rendering-integration.md"
  - related-to: "docs/stories/STORY-063-diagram-rendering-pipeline.md"
  - related-to: "docs/stories/STORY-047-table-widget-for-document-list.md"
  - related-to: "docs/stories/STORY-048-scroll-padding-and-half-page-navigation.md"
  - related-to: "docs/stories/STORY-049-scrollbar-for-focused-views.md"
---

# Rendering

`ui::draw(f, app)` is the top-level render function. It dispatches to mode-specific
renderers and overlays.

## Types Mode Layout

```d2
direction: right

screen: "Terminal" {
  left: "Type Selector\n(~20% width)" {
    style.fill: "#e8f0fe"
  }
  right: "Right Panel" {
    top: "Document List\n(~40% height)" {
      style.fill: "#e6f4ea"
    }
    bottom: "Document Preview\n(~60% height)" {
      style.fill: "#fff3e0"
    }
  }

  left -> right.top: "selected type filters list"
  right.top -> right.bottom: "selected doc shows preview"
}
```

The left panel shows document types with icons and counts. The right panel splits
into a document list (with tree indentation for children) and a preview pane showing
the selected document's body with expanded refs.

## Diagram Rendering

Documents containing d2 or mermaid code blocks get rendered as images in the
preview pane. See [STORY-063: Diagram rendering pipeline](../../stories/STORY-063-diagram-rendering-pipeline.md).

@ref src/tui/diagram.rs#DiagramBlock

The rendering pipeline:

1. `extract_diagram_blocks(body)` finds fenced blocks tagged `d2` or `mermaid`
2. `request_diagram_render()` checks cache, spawns render if missing
3. Rendering shells out to `d2` or `mmdc` CLI tools
4. Results are cached by source content hash
5. Images displayed via `ratatui-image` (protocol auto-detected)

Terminal protocol detection:

@ref src/tui/terminal_caps.rs#TerminalImageProtocol

When `ascii_diagrams = true` in config, or when image protocol is unsupported,
the raw source is displayed instead.
