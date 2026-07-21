---
title: TUI comment thread pane
type: story
status: draft
author: jack
date: 2026-07-21
tags: []
related:
- implements: RFC-060
---

## Story

As a TUI user, I want a thread/chat-like pane per document, so that I can read and post comments in context without leaving the terminal.

The TUI surface of the comment layer. Attribute-driven: it renders whatever attributes each comment carries, authored or adapter-sourced, with no hard-coded slot for `kind` or `confidence`.

## Scope

- A comment pane bound to the selected document: nested reply bubbles, author + timestamp headers, attribute chips per node.
- Collapse-resolved threads; jump-to-anchor (navigate to the `anchor` section slug).
- Inline reply: post a comment / reply from the pane.
- Renders from the same engine fold the CLI uses (TUI depends on engine, not CLI — convention principle 3).

Out of scope: web view (sibling story); any engine/CLI change (those shipped in earlier stories).

## Acceptance Criteria

- **Given** a document with a threaded discussion, **when** I open its comment pane, **then** replies render nested under their parents with author + relative time and attribute chips.
- **Given** a comment with an adapter-sourced `reactions` attribute, **then** its chip renders alongside authored chips — no hard-coded column; unknown attributes still display.
- **Given** a resolved thread, **when** collapse-resolved is active, **then** it collapses; expanding shows the full history.
- **Given** a comment with an `anchor`, **when** I jump-to-anchor, **then** the document view scrolls to that section slug.
- **Given** the pane, **when** I post a reply inline, **then** it appends via the engine and appears in the pane.

## Notes

Ships together with the web-view story (per CLAUDE.md: TUI, web, CLI move together). Both render the same folded tree; this story is the TUI half.

