---
title: "Unopinionated document types and relationships"
type: rfc
status: accepted
author: "jkaloger"
date: 2026-06-18
tags: ["config", "types", "relationships", "architecture"]
---

## Problem

Engine bakes the RFC→Story→Iteration ontology into code. `default_types()` (`src/engine/config.rs:351`) hardcodes 7 types. `default_rules()` (`config.rs:385`) hardcodes relationship names (`implements`) and parent-child chains. `Directories{rfcs,adrs,stories,iterations}` (`config.rs:244`) is a named-field struct that structurally assumes those 4 types exist. Relationship types are a closed enum: `RelationType` (`src/engine/document.rs:116`), 4 variants, ~89 references across 12 files spanning all three layers, with `FromStr`/`Display`/`resolve_rel_keyword` and inverse aliases all hardcoded.

Doc types are already open at the model level — `DocType(String)` newtype (`document.rs:54`) accepts any string; the opinionation is only the config fallback and `Directories`. Relationship types are not: the enum is closed, so a project can't add a relationship vocabulary without a code change.

## Intent

Engine carries no built-in document types, relationship types, relationship names, or validation rules. All declared in `.lazyspec.toml`. The sole home for defaults is the config `init` writes (and `fix` injects on migration). A fresh checkout's behavior is identical to today because `init` writes the same starter set; the difference is that the set lives in config, not code.

## Design

Resolved decisions (each has an ADR):

1. **Relationship model mirrors `DocType`.** `RelationType` becomes a `RelationType(String)` newtype validated against a config-loaded registry, exactly as doc types already work. No closed enum. (ADR-010)
2. **Vocabulary vs constraints split.** A relationship's *vocabulary* (name + inverse) lives in `[[relationships]]`. Type-pair *constraints* (which types may link with which relationship) stay in `[[rules]]`, referencing relationships by name. Relations are arbitrary doc→doc at the model level (`related: Vec<Relation>` on every `DocMeta`); constraints are a validation-layer concern only.
3. **Strict load, zero engine defaults.** Missing `[[types]]` or `[[relationships]]` is a hard error. No silent fallback in the load path. (ADR-011)
4. **Migration via `fix`.** `fix` gains a `--config` flag and a lenient config read (bypassing strict load) that injects the standard `[[relationships]]`/`[[rules]]` blocks. The strict-load error names `fix` as the remedy. (ADR-012)

Decided by precedent (mirror `DocType`):

- The newtype stays unvalidated as a value; `link` errors on an unknown relationship via registry lookup, and `validate` flags unknowns. `FromStr` stays pure (string → newtype), so the ~89-site refactor is mechanical.
- #55's inverse mechanism (store once, flip on display) is unchanged; only the *source* of inverse names moves from `INVERSE_STRS`/`resolve_rel_keyword` to the config `inverse` field.
- `Directories` named struct deleted; dirs derive from `types`. The `"story"→"stories"` pluralization helper dies with the defaults (`plural` is already a required `TypeDef` field).

`context` traversal already walks relationships generically (no type literals in `context.rs`), so the ontology removal does not touch chain traversal.

## Interface sketch

```toml
@draft [[relationships]]
name = "implements"
inverse = "implemented-by"   # omit => symmetric (e.g. related-to)

[[relationships]]
name = "related-to"

# constraints reference relationships by name, unchanged shape:
[[rules]]
shape = "parent-child"
child = "story"
parent = "rfc"
link = "implements"
```

```rust
@draft pub struct RelationType(String);              // mirrors DocType(String)

@draft pub struct RelationshipDef {                  // one per [[relationships]]
    name: String,
    inverse: Option<String>,
}
```

`@ref src/engine/document.rs#RelationType` — enum replaced by the newtype above.
`@ref src/engine/config.rs#Directories` — deleted; dirs derived from `types`.

## Stories

1. **Strict config-driven doc types.** Remove `default_types`/`directories_from_types`/`types_from_directories`, generalize away `Directories`, strict-load error for missing `[[types]]`. Model already open, so mostly deletion + error wiring.
2. **Relationship vocabulary as config.** `[[relationships]]` + `RelationshipDef` registry; `RelationType` enum → newtype; `link`/`unlink`/`validate` consume the registry; inverse sourced from config. The ~89-site change; ships atomically as one compile unit.
3. **`init` writes relationships + rules.** `init` currently scaffolds `[[types]]` only; add the standard `[[relationships]]` and `[[rules]]` blocks.
4. **`fix --config` migration.** Lenient config read, inject missing `[[relationships]]`/`[[rules]]`, strict-load error guides users to it.

## Open questions

None blocking. Sequencing note: story 2 is the heavy slice and should land before story 4 (migration must know the complete relationship set).
