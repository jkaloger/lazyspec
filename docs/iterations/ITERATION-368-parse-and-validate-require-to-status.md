---
title: Parse and validate require_to_status
type: iteration
status: in-progress
author: Jack Kaloger
date: 2026-08-31
tags: []
related:
- implements: STORY-255
- blocks: ITERATION-369
---

## Objective

`require_to_status` parses onto an edge as a per-target-type map, and a key naming a status absent from that type's lifecycle fails at load.

## Satisfies

STORY-255 AC5. AC1, AC2, AC3, AC4, AC6 deferred — see Out of scope.

## Context

- Story + ACs: STORY-255
- Why a map and not a scalar (a type set spans lifecycles; `bug` has no `accepted`): RFC-067 §Design, ADR-022
- `EdgeDef` as it stands: `src/engine/config.rs`, landed in commit `68e0a54`
- Existing per-type status validation to reuse: `validate_status` in `src/engine/config.rs`, called from `src/engine/ops/create.rs:101`
- Existing edge strict-load checks to sit beside: `src/engine/config.rs` `parse_inner`
- Conventions: `lazyspec convention`

## Tasks

1. Test-first: an edge with `require_to_status = { story = "accepted", bug = "triaged" }` parses into a map; an edge omitting the key parses with an empty map.
2. Add `require_to_status: BTreeMap<String, String>` to `EdgeDef`, defaulting empty, skipped when empty on serialize so `to_toml` stays clean — follow what `edges` itself does after `68e0a54`.
3. Test-first: a key naming a status absent from that target type's lifecycle fails load, naming the type, the status, and the edge. Then implement it beside the existing edge type/relationship checks.
4. Test-first: a `require_to_status` key naming a type absent from the edge's own `to` list fails load. The story does not state this, but a key that can never be read is a typo by construction and the same argument as AC5 applies. If this proves contentious, drop it rather than guessing — note it in the report.
5. Confirm `config --json` carries `require_to_status`, and extend the schema assertion.

## Out of scope

- Enforcing the gate at `create` (AC1, AC2, AC3, AC4, AC6) → next iteration on STORY-255.
- Removing the scalar `require_parent_status` gate → STORY-259, with `[[rules]]`.
- `"*"` wildcards → STORY-256. Traversal → STORY-257.

## Principles / conventions

`lazyspec convention`. Dictum 2 (`--json`), dictum 3 (engine owns it).

## Verification

A config whose edge gates a status the target type does not have fails load with a message naming all three of type, status, and edge — not a silent no-op at create time. That is the point of the slice: STORY-255's Notes call the load-time check the interesting part.
