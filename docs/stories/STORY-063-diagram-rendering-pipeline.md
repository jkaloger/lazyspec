---
title: Diagram rendering pipeline
type: story
status: review
author: jkaloger
date: 2026-03-13
tags:
- tui
- d2
- mermaid
related:
- implements: docs/rfcs/RFC-017-better-markdown-preview.md
---



## Context

The TUI preview renders d2 and mermaid fenced code blocks as plain text. RFC-017 calls for rendering these as inline images using the terminal's image protocol (sixel or kitty graphics), with async rendering, caching, and graceful fallback when tools or terminal support are unavailable.

## Acceptance Criteria

### AC1: Terminal image protocol detection

- **Given** the TUI is starting up
  **When** the terminal environment is inspected
  **Then** a capability is detected as one of: `Sixel`, `KittyGraphics`, or `None`

### AC2: Diagram code block detection

- **Given** a document body contains a fenced code block with language `d2` or `mermaid`
  **When** the preview renders that document
  **Then** the code block is identified as a diagram block (not rendered as plain code)

### AC3: Async diagram rendering

- **Given** a diagram code block is detected and the corresponding CLI tool (`d2` or `mmdc`) is installed
  **When** the preview encounters the block for the first time
  **Then** a loading indicator (`[rendering diagram...]`) is displayed and the CLI tool is invoked in a background thread

### AC4: Inline image display

- **Given** a diagram has been rendered to PNG and the terminal supports an image protocol
  **When** the rendering completes
  **Then** the PNG is displayed inline in the preview, replacing the code block

### AC5: Diagram caching

- **Given** a diagram block has been rendered previously
  **When** the same document is viewed again and the diagram source text has not changed
  **Then** the cached image is reused without re-invoking the CLI tool

### AC6: Cache invalidation

- **Given** a cached diagram exists for a code block
  **When** the diagram source text changes (detected by content hash)
  **Then** the cache entry is invalidated and the diagram is re-rendered

### AC7: Fallback when CLI tool is missing

- **Given** a d2 code block is detected but `d2` is not installed (or `mmdc` for mermaid)
  **When** the preview renders that block
  **Then** the code block is rendered with syntax highlighting (default behaviour) and a hint is shown: `[d2: install d2 CLI for diagram rendering]`

### AC8: Fallback when terminal lacks image support

- **Given** a diagram has been rendered to PNG but the terminal capability is `None`
  **When** the preview attempts to display the image
  **Then** the code block is rendered with syntax highlighting and a hint is shown: `[diagram: terminal does not support inline images]`

## Scope

### In Scope

- Terminal image protocol detection (sixel, kitty graphics)
- Detecting d2 and mermaid fenced code blocks during preview rendering
- Shelling out to `d2` / `mmdc` CLI to produce PNGs
- Displaying PNGs inline via the detected image protocol
- Content-hash-based caching of rendered images
- Async rendering with loading indicator
- Fallback to syntax-highlighted code block with informational hints

### Out of Scope

- Bundling or installing d2/mmdc CLI tools
- GFM extensions (separate story)
- Open-in-editor keybinding (separate story)
- Supporting image formats other than PNG
- Diagram editing or live-reload while editing diagram source
