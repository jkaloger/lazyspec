---
title: "Output Modes"
type: arch
status: accepted
author: jkaloger
date: 2026-03-15
tags: [architecture, cli, json]
related:
  - related-to: "docs/stories/STORY-021-json-everywhere.md"
  - related-to: "docs/stories/STORY-023-styled-cli-output.md"
---

# Output Modes

Every command that produces output supports two modes, driven by
[STORY-021: JSON Everywhere](../../stories/STORY-021-json-everywhere.md) and
[STORY-023: Styled CLI Output](../../stories/STORY-023-styled-cli-output.md).

## Human (default)

Uses the `console` crate for terminal styling.
- Status badges with colors (green=accepted, yellow=draft, blue=review, red=rejected, gray=superseded)
- Box-drawing characters for headers and cards
- Dim/bold emphasis
- Graceful fallback when colors are disabled

## JSON (`--json`)

Structured output for machine consumption.

@ref src/cli/json.rs

Consistent schema across all commands. Two serialization levels:
- `doc_to_json(doc)` -- basic document fields
- `doc_to_json_with_family(doc, store)` -- includes parent path and child paths
