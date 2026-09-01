---
title: Per-edge traversal role, rejected when two rows disagree
type: iteration
status: complete
author: jack
date: 2026-08-31
tags: []
related:
- implements: STORY-257
- blocks: ITERATION-373
- blocks: ITERATION-376
- blocks: ITERATION-387
- blocks: ITERATION-389
- blocks: ITERATION-392
- blocks: ITERATION-393
- blocks: ITERATION-396
- blocks: ITERATION-395
---

## Objective

An `[[edges]]` row carries `traversal = "chain" | "related"`, and config load fails -- naming both rows -- when two rows that can match the same concrete edge assign it different traversal roles. Nothing walks differently yet.

## Satisfies

STORY-257 AC3. AC1, AC2, AC4, AC5, AC6 deferred -- see Out of scope.

## Context

- Story + ACs: STORY-257
- Why traversal belongs on the triple rather than the relationship name: RFC-067 §Problem.3, ADR-030 §Decision
- "Traversal composes; two rows assigning different roles to the same triple is a load error, not a precedence puzzle": ADR-031 §Decision
- The overlap predicate and specificity scoring this check reuses landed in ITERATION-370, which names this slice as their second consumer
- Touch:
  - `src/engine/config.rs` -- `EdgeDef` gains `traversal`; the strict-load edge loop in `Config::parse` gains the contradiction check; the JSON-schema assertion and the `to_toml` round-trip
  - `README.md` §`[[edges]]` -- the closing paragraph currently asserts per-edge `traversal` is "not supported yet"
- `Traversal` already exists and is already serialised on `RelationshipDef`. This slice adds the field to `EdgeDef` and removes nothing. No consumer reads the new field until the next slice, so `context`, `validate`, the TUI and the web view are unchanged here by construction, not by care.

## Tasks

1. Test-first: `traversal = "chain"`, `"related"`, and an absent key on an `[[edges]]` row parse to `Some(Chain)`, `Some(Related)`, `None`, and round-trip through `to_toml` with absence omitted -- the same `skip_serializing_if` spelling `RelationshipDef` already uses.
2. Add `traversal: Option<Traversal>` to `EdgeDef`, reusing the existing enum. A parallel enum for the same two roles would turn STORY-258's migration from a move into a translation.
3. Test-first: two overlapping rows carrying different `Some(Traversal)` values fail load with a message naming both `name`s. Cover equal **and** unequal specificity -- unlike requiredness, traversal does not resolve by specificity, so a concrete row disagreeing with a wildcard row is an error too, not a most-specific-wins case.
4. Test-first: two overlapping rows where one declares a role and the other declares none load fine and each keeps its own value. An absent `traversal` means "joins no walk", which is not a disagreement -- the same conclusion ADR-031 reaches for an absent `required`: a row silent on a key states nothing about it and so conflicts with nothing.
5. Wire the check into `Config::parse` beside ITERATION-370's requiredness checks, reusing its overlap predicate rather than writing a second one.
6. Extend the JSON-schema assertion to cover `EdgeDef.traversal` and confirm `config --json` carries it.
7. README §`[[edges]]`: document `traversal` as the sixth key, state that roles compose across matching rows while a disagreement is a load error, and drop the "per-edge `traversal` is not supported yet" clause. Leave §`[[relationships]]`'s `traversal` paragraph alone -- it is still the only thing driving the walk until the next slice.

## Out of scope

- Anything that reads the field: the chain walk (AC1, AC2), the related walk (AC4), surface parity (AC6). `Store` still derives `chain_relationships`/`related_relationships` from `RelationshipDef.traversal` exactly as today.
- Removing `traversal` from `RelationshipDef` -> STORY-259, with the rest of the dual-declaration window.
- `fix --config` emitting `traversal` rows (AC5) -> STORY-258.

## Principles / conventions

`lazyspec convention` and the dictums it lists. Dictum 6: reuse `Traversal` and ITERATION-370's overlap predicate rather than introducing parallels. Dictum 2: the new key must survive `config --json` and the emitted schema.

## Verification

`cargo run -- config --json | jq .edges` still returns `[]` on this repo, and `cargo run -- context STORY-257 --json` is byte-identical before and after the slice.
