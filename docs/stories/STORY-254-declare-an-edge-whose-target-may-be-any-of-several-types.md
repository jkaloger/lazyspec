---
title: Declare an edge whose target may be any of several types
type: story
status: accepted
author: Jack Kaloger
date: 2026-08-29
tags: []
related:
- implements: RFC-067
- blocks: STORY-255
- blocks: STORY-256
- blocks: STORY-257
- blocks: STORY-258
- blocks: STORY-260
- blocks: STORY-261
---
As a DAG designer, I want a single config row to declare that an iteration implements a spike, a story, *or* a bug, so that the constraint I actually mean becomes expressible.

This is the walking skeleton for RFC-067: the thinnest path that parses an `[[edges]]` row and enforces it end-to-end. `[[rules]]` keeps working alongside, so nothing breaks while the rest of the table is built out. STORY-259 closes that window.

## Scope

In: `EdgeDef` with `from`, `to`, `via`, `required`; strict-load validation of type and relationship names; the validation checker; `config --json` exposure.

Out: `traversal` (STORY-257), wildcards (STORY-256), `require_to_status` (STORY-255), migration (STORY-258), editors (STORY-260, STORY-261).

## Acceptance criteria

- Given an edge with `from = "iteration"`, `to = ["spike", "story", "bug"]`, `via = "implements"`, `required = "error"`, when an iteration links `implements` to a document of any one of those three types, then `validate` reports no finding for that edge.
- Given the same edge, when an iteration carries no `implements` link to any of the three, then `validate` reports one error naming the edge by `name` and listing all three permitted target types.
- Given the same edge, when an iteration links `targets` (not `implements`) to a story, then `validate` still reports the error — a relationship other than `via` does not satisfy the edge. This is the defect described in RFC-067 §Problem.1.
- Given `to = ["story"]`, when written as the scalar `to = "story"`, then it loads identically to the single-element list.
- Given an edge naming a type or relationship absent from config, when the config loads, then load fails with an error naming the unknown identifier and the offending edge.
- Given any config, when `lazyspec config --json` runs, then declared edges appear in the output with every field.
- Given a config carrying both `[[rules]]` and `[[edges]]`, when `validate` runs, then both are enforced and neither suppresses the other.

## Notes

The finding must name the whole target set, not just the first member: "an iteration needs a story" is the wrong message when spikes and bugs are equally valid.

Per dictum 2, `--json` is not optional. Per dictum 3, `EdgeDef` and its checker live in the engine; the CLI only formats.
