---
title: Configure the staleness bands and the per-type driver
type: iteration
status: complete
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- implements: STORY-272
- blocks: ITERATION-422
---

## Objective

`[staleness]` thresholds and a per-type `staleness` driver key load, round-trip through the config writer, and reach the schema.

## Satisfies

STORY-272, no AC on its own. AC3's thresholds and AC4's driver read this config; the compute slice satisfies them.

## Context

- Story + ACs: STORY-272
- Shape, defaults and which types get `drift`: RFC-069 §Design "Configuration"
- Touch:
  - `src/engine/config.rs:1244` `GovernsConfig` is the precedent for a new global table: struct, `default_*` fns, `Default` impl, field on `Config` (`:1137`), field on the raw config (`:1362`), unwrap in the raw-to-`Config` conversion (`:1902`).
  - `src/engine/config.rs:823` `status_authority` is the precedent for a new `[[types]]` key: `#[serde(default)]` on `TypeDef`, every `TypeDef` literal in tests, the writer at `src/engine/config_write.rs:101`, the round-trip test at `config.rs:4779` and the decor test at `config_write.rs:1539`.
  - `.lazyspec.toml` -- spec, convention and dictum get `staleness = "drift"`. RFC and ADR stay default.
  - `README.md:970` documents `[governs]`; `[staleness]` goes beside it, plus the per-type key in §Custom types.
- `aging` and `stale` are `"90d"` strings. No duration crate is in `Cargo.toml` and none is being added -- parse `<n>d` and error on anything else. The band table only ever compares whole days, so days is all the parse has to yield.
- Driver is an enum with two values, not a bool and not a string. Default is `age`; an unknown value is a config error, the way every other enum key in this file behaves.

## Tasks

1. Test-first: the table parses `"90d"`/`"180d"`; an absent table gives the same defaults; `aging = "ninety"` is a config error, not a panic.
2. Test-first: `staleness = "drift"` on a `[[types]]` entry parses, absent is `age`, an unknown value errors.
3. Implement both, following `GovernsConfig` and `status_authority`.
4. Writer: the type key survives a rewrite with its decor, and setting it writes the key.
5. `.lazyspec.toml`: spec, convention, dictum to `drift`.
6. README: the `[staleness]` table and the per-type key.

## Out of scope

- `[staleness].finding` -- STORY-273, with the validation rule it configures. Nothing in this story reads it.
- `compute`, `show`, `why`. Nothing reads either key when this lands, by design.
- Stamping `reviewed` -- STORY-274.

## Principles/conventions

`cargo run --quiet -- convention`. DICTUM-005: `<n>d` is a dozen lines, not a dependency.

## Verification

`cargo run --quiet -- config show --json | jq '.staleness, (.documents.types[] | select(.name=="spec") | .staleness)'` gives the thresholds and `"drift"`.
