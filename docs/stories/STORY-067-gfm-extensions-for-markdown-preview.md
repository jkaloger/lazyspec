---
title: GFM extensions for markdown preview
type: story
status: accepted
author: jkaloger
date: 2026-03-18
tags:
- tui
- markdown
- gfm
related:
- implements: docs/rfcs/RFC-017-better-markdown-preview.md
---



## Context

The TUI preview panel renders markdown using `tui_markdown::from_str()` with default options. This means GFM extensions like tables, admonitions, and footnotes are parsed as plain text rather than rendered with their intended formatting. The underlying `pulldown-cmark` parser supports these features via option flags, but they aren't enabled.

## Acceptance Criteria

- **Given** a document contains a GFM table (pipe-delimited rows with a header separator)
  **When** the document is displayed in the TUI preview panel
  **Then** the table renders with aligned columns and visible cell borders

- **Given** a document contains a GFM admonition block (`> [!NOTE]`, `> [!WARNING]`, `> [!TIP]`, `> [!IMPORTANT]`, `> [!CAUTION]`)
  **When** the document is displayed in the TUI preview panel
  **Then** the admonition renders with its type label visible and content visually distinguished from surrounding text

- **Given** a document contains footnote references and definitions (`[^1]` / `[^1]: ...`)
  **When** the document is displayed in the TUI preview panel
  **Then** footnote references are visually indicated and definitions are rendered at the bottom of the document

- **Given** a document contains strikethrough text (`~~deleted~~`)
  **When** the document is displayed in the TUI preview panel
  **Then** the text renders with strikethrough styling

- **Given** a document contains task list items (`- [ ]` / `- [x]`)
  **When** the document is displayed in the TUI preview panel
  **Then** checkboxes render as empty or checked indicators

## Scope

### In Scope

- Enabling `pulldown-cmark` GFM option flags in the markdown preview renderer
- Table rendering with column alignment
- Admonition block rendering with type labels
- Footnote rendering
- Strikethrough and task list rendering (verifying these work with the new options)

### Out of Scope

- Diagram rendering (d2/mermaid) -- covered by STORY-063
- Open-in-editor keybinding -- separate story under RFC-017
- Custom table styling or interactive table features
- Editing markdown from within the preview
