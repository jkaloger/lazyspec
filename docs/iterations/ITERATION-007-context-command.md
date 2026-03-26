---
title: Context Command
type: iteration
status: accepted
author: agent
date: 2026-03-05
tags: []
related:
- implements: STORY-019
---




## Changes

- Added `src/cli/context.rs` with `resolve_chain`, `run_json`, `run_human`
- Added `Context` subcommand to CLI with `id` arg and `--json` flag
- Chain walking follows `implements` links upward from leaf to root via `Store::get()`
- JSON output wraps chain in `{ "chain": [...] }` using shared `doc_to_json` schema
- Human-readable output shows title, type, status, path with arrow connectors

## Test Plan

- `context_walks_full_chain` — AC1: Iteration -> Story -> RFC chain in correct order
- `context_standalone_document` — AC2: RFC with no implements returns single-element chain
- `context_json_output` — AC3: JSON with chain array using consistent schema
- `context_human_output` — AC4: human-readable with all titles and types
- `context_not_found` — AC5: error on nonexistent document

## Notes

All 5 Story ACs covered.
