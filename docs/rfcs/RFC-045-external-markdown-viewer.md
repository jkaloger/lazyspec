---
title: "External markdown viewer"
type: rfc
status: superseded
author: "jkaloger"
date: 2026-06-18
tags: []
---

## Problem

Markdown rendering is internal (RFC-033 unified pipeline; RFC-017 preview). No escape hatch to delegate to an external viewer (`glow`, `nvim`, `bat`, `$PAGER`). Users with established viewer preferences can't substitute.

## Intent

Config to disable the internal viewer and delegate rendering to an external command. CLI `show` and the TUI reader hand the document to the configured viewer; absent config, internal renderer stays the default.

## Sketch

```toml
@draft [viewer]
external = "glow -"        # command; "-" reads markdown from stdin
# unset => internal renderer (current behavior)
```

- `@ref src/cli/show.rs` — when `viewer.external` set, pipe document to the command's stdin (or temp file + path arg for viewers needing a path).
- TUI reader: suspend TUI, spawn viewer, resume on exit — mirror existing external-editor spawn (`src/tui/views/keys.rs`).

Standalone: no dependency on the other three RFCs. Smallest blast radius.

## Stories

- `[viewer]` config + external-command spawn for CLI `show`.
- TUI reader delegates to external viewer (suspend/resume).

## Open questions

- Hand off raw markdown vs lazyspec-rendered (`@ref` resolved)?
- stdin pipe vs temp file — support both?
- Resolve `@ref` directives before handing off, or pass verbatim?

