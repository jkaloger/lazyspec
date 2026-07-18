---
title: "Wrap mode measures unconfigured tag/provenance columns, inflating row heights"
type: bug
status: triaged
author: "unknown"
date: 2026-07-18
tags: []
related: []
---

## Summary

In wrap mode, TUI list rows grow taller than needed: row height calculation measures tag and provenance column wrap heights even when those columns are not in the configured column set. Non-default column configs only.

## Reproduction

1. Configure TUI columns without tags/provenance.
2. Enable wrap mode.
3. View a doc with many tags or provenance entries — row takes extra vertical lines despite columns not rendering.

## Expected

Row height reflects only configured, rendered columns.

## Actual

`row_content_lines` (src/tui/views/panels.rs:449, called at :818) measures tags and provenance wrap heights unconditionally, so hidden columns still contribute to the row's line count.

## Fix direction

Gate tag/provenance measurement on the columns actually configured — height calculation and rendering must consult the same column set.
