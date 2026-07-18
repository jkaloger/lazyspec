---
title: "show --open --json prints human text on ambiguous ID instead of JSON error"
type: bug
status: triaged
author: "unknown"
date: 2026-07-18
tags: []
related: []
---

## Summary

`lazyspec show <id> --open --json` on an ambiguous shorthand prints human-readable text to stderr, while plain `show <id> --json` returns the machine-readable `{"error": "ambiguous_id", ...}` shape. Agents driving `--open --json` can't detect ambiguity programmatically.

## Reproduction

1. Two docs whose shorthands collide (e.g. same number across types resolved loosely).
2. `lazyspec show <ambiguous> --json` → JSON error object.
3. `lazyspec show <ambiguous> --open --json` → "Ambiguous ID ... Specify the full path" on stderr, exit 0, no JSON.

## Expected

`--open --json` emits the same `{"error": "ambiguous_id", "ambiguous_matches": [...]}` shape as `run_json`.

## Actual

`run_open` (src/cli/show.rs:214) handles `ResolveError::Ambiguous` with `eprintln!` before the `json` flag is consulted; `run_json` (src/cli/show.rs:182) has the correct JSON branch.

## Fix direction

In `run_open`, when `json` is set, print the same JSON error object as `run_json` and return.
