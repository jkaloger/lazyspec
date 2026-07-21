---
title: Web-view comment thread
type: story
status: rejected
author: jack
date: 2026-07-21
tags: []
related:
- implements: RFC-060
---

## Story

As a web-view user, I want the comment thread rendered on a document page, so that I can read the discussion in the browser view alongside the TUI and CLI.

The web surface of the comment layer, sibling to the TUI pane. Same folded tree, attribute-driven rendering.

## Scope

- Web view renders the folded comment tree per document: nested replies, author + timestamp, attribute chips.
- Resolved-state shown; attribute chips cover authored and adapter-sourced attributes with no hard-coded slot.
- Renders from the same engine fold (web depends on engine).

Out of scope: TUI pane (sibling story). Whether the web view is read-only or supports posting follows the web view's existing capability for other document operations — match it, do not invent a one-off write path here.

## Acceptance Criteria

- **Given** a document with a threaded, partly-resolved discussion, **when** I open its web page, **then** the thread renders as a nested tree with author, timestamp, resolved-state, and attribute chips.
- **Given** a comment carrying an adapter-sourced attribute (e.g. `reactions`), **then** it renders as a chip alongside authored attributes — no hard-coded `kind`/`confidence` slot.
- **Given** the same document, **then** the web tree and the TUI pane render the same folded structure (parity).
- **Given** the web view's existing interaction model, **then** comment rendering matches it (read-only vs interactive) rather than introducing a bespoke path.

## Notes

Per CLAUDE.md the three surfaces move together; this is the web half of the pairing with the TUI thread pane. All surfaces are attribute-driven.

