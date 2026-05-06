---
title: Markdown newline preservation
type: story
status: draft
author: jkaloger
date: 2026-04-30
tags: []
related:
- implements: RFC-040
---


## Context

RFC-040 (TUI Multi-Line Display) specifies that `render_gfm_segments` must preserve intentional line breaks from markdown source, not just paragraph breaks. Currently, soft wraps are handled by `Paragraph::wrap`, but hard newlines (explicit `\n` in source) may not render as actual line breaks in the TUI preview.

## Acceptance Criteria

- **AC1:** Given markdown content with explicit newlines (`\n`), When rendered in TUI preview, Then line breaks are preserved as visible line breaks

- **AC2:** Given an admonition block with multi-line content, When rendered, Then internal newlines are preserved within the admonition display

- **AC3:** Given a code block with newlines, When rendered, Then newlines are preserved as part of code block formatting

- **AC4:** Given mixed content (paragraphs with blank line breaks + explicit newlines within paragraphs), When rendered, Then both paragraph breaks and explicit line breaks render correctly

- **AC5:** Given content with lines exceeding terminal width, When rendered, Then soft wrap works via `Paragraph::wrap` while hard newlines remain preserved

## Scope

### In Scope

- Updating `render_gfm_segments` to preserve hard newlines in markdown source
- Verifying admonition rendering preserves internal newlines
- Verifying code block rendering preserves newlines
- Ensuring `Paragraph::wrap` handles soft wraps without affecting hard newlines
- Testing with mixed content scenarios

### Out of Scope

- Changing terminal width detection or dynamic reflow behavior
- Modifying markdown parsing logic (pulldown-cmark integration)
- Adding new TUI components or widgets beyond current `Paragraph`-based rendering
