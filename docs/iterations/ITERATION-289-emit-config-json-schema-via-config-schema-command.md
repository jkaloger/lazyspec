---
title: Emit config JSON Schema via config schema command
type: iteration
status: in-progress
author: jkaloger
date: 2026-07-10
tags: []
related:
- implements: STORY-204
---

## Objective

`lazyspec config schema` emit JSON Schema for `.lazyspec.toml`, derived from parse-path structs.

## Satisfies

STORY-204 AC1–AC6.

## Context

- Story + ACs: STORY-204
- Design: RFC-058 (schema source of truth = `RawConfig`, not `Config`)
- Conventions: docs/convention (layering principle 3, ecosystem norms principle 5)
- Touch: `Cargo.toml` (add `schemars` 1.x direct — already in lock via serde_with), `src/engine/config.rs` (`RawConfig` at 641 + nested types: `TypeDef`, `Lifecycle`, `Edge`, `RelationshipDef`, `ValidationRule`, `Severity`, `Traversal`, `NumberingStrategy`, `Authorship`, `AttrDef`, `AttrKind`, `StoreBackend`, `ReservedConfig`, store/ui/github/web/agents sub-structs), `src/cli/config.rs` (`ConfigCommand` enum), `src/main.rs` (dispatch ~454), `README.md` (config inspection section ~507)

## Tasks

1. Add `schemars = "1"` to Cargo.toml.
2. Derive `JsonSchema` on `RawConfig` + every type it references transitively. Serde attrs already correct — schemars honor them.
3. Doc comments on top-level `RawConfig` fields + all `TypeDef` fields (condense from README config sections) → schema `description`s.
4. Engine fn `config_schema() -> schemars::Schema` in `src/engine/config.rs`. `RawConfig` stay private.
5. CLI: `ConfigCommand::Schema` variant, print schema pretty JSON to stdout. Works without project. `--json` flag accepted, same output.
6. Tests: schema is valid JSON + has `properties.types`; repo's own `.lazyspec.toml` (toml→json) validates against schema (use `jsonschema` dev-dep or assert structural spot-checks: `shape` tag enum, `severity` enum values, kebab store names); invalid enum value fails.
7. README: document `config schema` under config inspection section.

## Out of scope

- CI release-asset publish + `init` writing `#:schema` header → follow-up story (RFC-058 story 3)
- Doc comments beyond top-level + TypeDef (rest of structs best-effort, not gated)
- SchemaStore, runtime schema validation

## Verification

`cargo run -- config schema | jq .` succeed outside repo dir. Schema `$defs` contain `ValidationRule` variants keyed by `shape`. `cargo test` green, `cargo fmt --check`, `cargo clippy` clean.

