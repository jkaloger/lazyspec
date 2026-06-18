---
title: Relationship types as validated string newtype with config registry
type: adr
status: accepted
author: jkaloger
date: 2026-06-18
tags:
- config
- relationships
- architecture
related:
- related-to: RFC-042
---

## Context

`RelationType` is a closed Rust enum (`src/engine/document.rs:116`): 4 variants, ~89 references across 12 files. To let projects declare their own relationship vocabulary, the set must be defined by config at runtime, not by an enum at compile time. Doc types already solved this: `DocType(String)` newtype (`document.rs:54`) accepts any string, validated by rules rather than by the type system.

## Decision

Replace the `RelationType` enum with a `RelationType(String)` newtype, mirroring `DocType`. Relationship vocabulary is declared in `[[relationships]]` as `RelationshipDef { name, inverse }` and loaded into a registry on `Config`. The newtype stays unvalidated as a value; `link` rejects an unknown relationship via registry lookup and `validate` flags unknowns. `FromStr` stays pure (string → newtype). Type-pair constraints are out of scope here — they remain in `[[rules]]` (see RFC-042).

Rejected: a hybrid `enum + Other(String)` (two representations of one value, defeats exhaustiveness) and an interned handle (compiler-grade machinery with no measured perf need across pass-through sites; violates principle 6).

## Consequences

- Doc types and relationship types are modeled identically — one mental model.
- The ~89-site change is mechanical because parsing stays pure and validation is a separate registry check.
- Validity is a runtime/validation property, not a compile-time guarantee; unknown relationships surface at `link` and `validate`, consistent with how unknown doc types behave today.
