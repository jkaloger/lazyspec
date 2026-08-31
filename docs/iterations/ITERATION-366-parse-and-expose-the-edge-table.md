---
title: Parse and expose the edge table
type: iteration
status: in-progress
author: Jack Kaloger
date: 2026-08-31
tags: []
related:
- implements: STORY-254
- blocks: ITERATION-367
---

## Objective

`[[edges]]` rows parse into an `EdgeDef`, are rejected at load when they name an unknown type or relationship, and appear in `config --json` and the emitted JSON schema.

## Satisfies

STORY-254 AC4, AC5, AC6. AC1, AC2, AC3, AC7 deferred — see Out of scope.

## Context

- Story + ACs: STORY-254
- Table shape and field semantics: RFC-067 §Interface sketch, §Design
- Conventions: `lazyspec convention` — dictum 2 (`--json` is not optional), dictum 3 (engine owns the type, CLI only formats), dictum 6 (no indirection before a second use)
- Touch:
  - `src/engine/config.rs` — `EdgeDef` beside `ValidationRule` (L27-47); `Config.edges` (L638); the raw loader struct (L808-811); strict-load checks (L1082-1190); the schema assertion (L1469-1487)
  - `src/cli/config.rs` — `--json` already serialises `Config`; confirm edges land and cover it

## Tasks

1. Test-first: `to = "story"` (scalar) and `to = ["story"]` (list) deserialise identically.
2. Add `EdgeDef` with `name`, `from: String`, `to: Vec<String>` (scalar-or-list serde), `via: String`, `required: Option<Severity>`. Concrete `String`/`Vec<String>` fields, **not** the RFC's `TypeSelector`/`RelSelector` enums — those exist to carry `"*"`, which arrives in STORY-256. Per dictum 6, introduce them at that second use.
3. Add `edges: Vec<EdgeDef>` to `Config` and the raw loader struct, defaulting empty so every config that predates the table still loads.
4. Test-first: an edge naming an unknown type or unknown relationship fails load with a message carrying both the unknown identifier and the offending edge's `name`. Then implement it in the strict-load path alongside the existing type/relationship checks.
5. Test: `config --json` carries declared edges with every field populated, and extend the existing JSON-schema assertion to cover `EdgeDef`.

## Out of scope

- The validation checker that enforces an edge (AC1, AC2, AC3) and rules/edges coexistence (AC7) → next iteration on STORY-254.
- `traversal` on the edge → STORY-257.
- `require_to_status` → STORY-255.
- `"*"` endpoints, the selector enums, and rejecting `required` on a wildcard `from` → STORY-256.
- `fix --config` migration → STORY-258. Retiring `[[rules]]` → STORY-259. Editors → STORY-260, STORY-261.

## Principles / conventions

`lazyspec convention` and the dictums it lists. Test-driven per the project's testing convention.

## Verification

`cargo run -- config --json | jq .edges` returns `[]` on this repo (which declares no edges yet), not `null` and not an error.
