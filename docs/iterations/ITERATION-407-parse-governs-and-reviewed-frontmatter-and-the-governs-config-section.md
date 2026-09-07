---
title: Parse governs and reviewed frontmatter and the governs config section
type: iteration
status: draft
author: Jack Kaloger
date: 2026-09-07
tags: []
related:
- implements: STORY-265
- blocks: ITERATION-408
- blocks: ITERATION-410
- blocks: ITERATION-418
---

## Objective

`governs` and `reviewed` load off every document; `[governs]` loads off `.lazyspec.toml`. Nothing consumes them yet.

## Satisfies

STORY-265 AC6. Substrate for AC1-AC5 and AC7.

## Context

- Story + ACs: STORY-265
- Field shape, config keys, defaults, `GovernsConfig`: RFC-068 §Design "Frontmatter" and "Configuration", §Interfaces
- Touch:
  - `src/engine/document.rs:311` `DocMeta`, `:347` the raw deserialise struct, `:455` per-entry load validation, `:491` the mapping, `:529` the test default. `provenance` is the pattern to copy -- a list field with an empty default and no finding when absent.
  - `src/engine/config.rs:1099` `Config` -- `governs: GovernsConfig` beside `certification` (`:1146`) and `git_ref` (`:1217`), which are the section-struct shape.
  - `Cargo.toml` -- `globset`, named in RFC-068 §Interfaces. DICTUM-005 wants no second crate for a job an existing one does; confirm nothing here globs already before adding it.
- `unowned` is `Option<Severity>`, `None` = off. Reuse the severity type validation already splits errors from warnings with; RFC-068 does not invent one.
- Every `DocMeta` construction site moves: `clickup_cache.rs:272`, `github_url.rs:187`, `show.rs:358`, and the TUI fixture at `panels.rs:3177`.

## Tasks

1. Test-first in `document.rs`: `governs` loads in order, missing key gives an empty vec, `reviewed` missing gives `None`. Mirror `provenance_loads_in_order` (`:570`) and `provenance_missing_defaults_empty` (`:598`).
2. Add both fields to `DocMeta` and its raw struct; fix the construction sites, deriving `Default` where it reaches.
3. Add `globset` to `Cargo.toml`.
4. Test-first in `config.rs`: `[governs]` with all three keys loads; an absent section gives `scope: []`, `unowned: None`, `root: "."`.
5. Add `GovernsConfig`, wire it onto `Config`.

## Out of scope

- Compiling or matching globs, `governing()` -- the next iteration. `globset` lands unused here.
- Any validation rule, `why`, `show` output, TUI, web.
- Judging `reviewed`. RFC-069 owns that; here it is a parsed string.

## Principles/conventions

`cargo run --quiet -- convention`. DICTUM-005 governs the new dependency.

## Verification

Add `governs: ["src/engine/**"]` and a `reviewed` sha to a document in this repo, run `cargo run --quiet -- validate --json`: the finding set is unchanged and the load does not error (AC6). Revert.
