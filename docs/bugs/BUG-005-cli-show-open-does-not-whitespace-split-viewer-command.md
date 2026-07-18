---
title: "CLI show --open does not whitespace-split viewer command"
type: bug
status: fixed
author: "unknown"
date: 2026-07-18
tags: []
related: []
---

## Summary

`viewer = "code -w"` in config works in the TUI but fails via `lazyspec show <id> --open`: the CLI treats the whole viewer string as one binary name.

## Reproduction

1. Set `viewer = "code -w"` (any viewer with args) in config.
2. `lazyspec show <id> --open` on a file-backed doc.
3. Fails: "failed to launch viewer 'code -w'".

## Expected

Viewer string split on whitespace — first token binary, rest args — matching TUI behaviour (src/tui/state/app.rs:312 splits).

## Actual

`spawn_open` (src/cli/show.rs:291) does `Command::new(&command).arg(&path)` with the unsplit string.

## Fix direction

Split viewer on whitespace in `spawn_open` (or in `plan_open` so the split is testable), same treatment $EDITOR got in the BUG-001 fix.
