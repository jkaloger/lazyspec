---
title: Render hydra interview bodies
type: iteration
status: accepted
author: Jack Kaloger
date: 2026-08-17
tags: []
related:
- implements: STORY-252
- blocks: ITERATION-364
---

## Objective

`lazyspec show` on a hydra document displays the intent, the ASCII tree, and every decision with its rationale and rejected alternatives.

## Satisfies

STORY-252 AC2, AC4. Depends on ITERATION-362.

## Context

- Story + ACs: STORY-252
- Body layout, and why rationale/rejected are included: RFC-066 §Body rendering
- Reference rendering to match: `hydra tree` output, and the tree shown in `.hydra/hydra-store.json`
- Touch: `src/engine/store/hydra.rs` (`render_body`, `render_ascii`)

## Tasks

1. Implement `render_ascii` over the parsed heads: spanning tree by parent, document order, answered/open/cauterised markers, and `blocked by <slugs>` annotations. Match `hydra tree`'s shape closely enough that a reader recognises it; exact glyph parity is not required.
2. Implement `render_body` to the layout in RFC-066 §Body rendering.
3. Place cauterised heads under Decisions with their `cauterised_by`, never under Open questions.
4. Escape or fence any answer text that would break the surrounding markdown — answers are free prose and routinely contain backticks and fenced blocks.
5. Snapshot-style tests over a fixture tree covering: an answered head with rejected alternatives, a cauterised head, a blocked open head, and an empty tree.

## Out of scope

- Any change to id or status derivation (ITERATION-362).
- TUI or web specific rendering — both consume the same body.
- Caching the rendered body. RFC-066 §Risks accepts per-load rendering until measured.

## Principles/conventions

`lazyspec convention`. Rendering belongs in the engine, not the TUI or CLI layer.

## Verification

`cargo run -- show HYDRA-HYDRA-STORE` renders 20 decisions and 1 cauterised head, and the markdown fences in the output are balanced.

