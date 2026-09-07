---
title: Look up governing documents from the TUI and web
type: story
status: complete
author: Jack Kaloger
date: 2026-09-07
tags: []
related:
- implements: RFC-068
---

As a reader browsing documents in the TUI or web view, I want to see a document's pins and to type a file path into search to find its governing documents, so that the lookup `why` gives the CLI is available without leaving the browser.

## Acceptance criteria

- Given a document with `governs` and `reviewed`, when I open its detail in the TUI, then both fields render alongside the other frontmatter fields.
- Given the same document, when I open its detail in the web view, then both fields render.
- Given a file path typed into TUI fuzzy search (`/`), when documents have globs matching that path, then those documents appear in the results.
- Given the same path typed into the web search box, then the same documents appear.
- Given a query that matches both a title and a path, when results render, then both sets appear; path matches are additive to text matches.
- Given a document with no pins, when detail renders, then no empty `governs` or `reviewed` row is shown.

## Notes

No new screen, column or graph mark, per RFC-068 §Non-goals. Matching goes through the engine's `governing` function from the walking skeleton; the TUI and web only call it.
