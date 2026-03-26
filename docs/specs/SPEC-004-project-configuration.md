---
title: "Project Configuration"
type: spec
status: draft
author: "jkaloger"
date: 2026-03-25
tags: [engine, config]
related: []
---

## Summary

All project configuration lives in a single `.lazyspec.toml` file at the repository root. Every field is optional. When the file is absent or a field is omitted, the system applies built-in defaults that reproduce the original hardcoded behavior. The configuration surface covers document type definitions, validation rules, numbering strategies, naming patterns, filesystem paths, and UI preferences.

## Config Loading

@ref src/engine/config.rs#Config

The `Config::load` method checks for `.lazyspec.toml` at the project root via the injected `FileSystem` trait. If the file exists, its contents are passed to `Config::parse`; otherwise `Config::default()` produces the full default configuration. There is no config merging: the file either exists and is parsed in full, or defaults apply wholesale.

@ref src/engine/config.rs#RawConfig

Parsing uses a `RawConfig` intermediate struct. TOML is deserialized into `RawConfig` first, then the `parse` method resolves fallbacks field-by-field. This two-phase approach lets individual sections default independently: a file that only sets `[[rules]]` still gets default types, naming, and templates.

## Type Definitions

@ref src/engine/config.rs#TypeDef

Each document type is a `TypeDef` with required fields `name`, `plural`, `dir`, and `prefix`, plus optional `icon`, `numbering` (defaulting to `Incremental`), and `subdirectory` (defaulting to `false`). The `subdirectory` flag controls whether the type stores documents as directories containing an `index.md` rather than flat files.

When `[[types]]` is present in the TOML, those definitions replace the defaults entirely. When absent, the parser checks for a legacy `[directories]` section and synthesizes types from it via `types_from_directories`. If neither is present, `default_types` provides five built-in types: rfc, story, iteration, adr, and spec. The spec type is the only default with `subdirectory = true`.

@ref src/engine/config.rs#default_types

The legacy path (`types_from_directories`) only produces four types (rfc, story, iteration, adr) since the old `Directories` struct has no spec field. This means projects relying on the legacy `[directories]` section do not get the spec type.

## Numbering Strategies

@ref src/engine/config.rs#NumberingStrategy

The `NumberingStrategy` enum has three variants: `Incremental` (the default), `Sqids`, and `Reserved`. Each type definition carries its own numbering strategy, set per-type in the TOML.

### Sqids

@ref src/engine/config.rs#SqidsConfig

When any type uses `Sqids` numbering, a `[numbering.sqids]` section must be present with a non-empty `salt`. The `min_length` field defaults to 3 and must fall between 1 and 10 inclusive. Validation at parse time rejects configurations that reference sqids numbering without the required section or with out-of-range values.

### Reserved

@ref src/engine/config.rs#ReservedConfig

The `Reserved` strategy coordinates number allocation across distributed contributors via git refs. Its configuration requires a `[numbering.reserved]` section with a `format` field (either `incremental` or `sqids`) and optional `remote` (defaults to `"origin"`) and `max_retries` (defaults to 5). When `format` is `sqids`, the parser additionally validates that a `[numbering.sqids]` section with a valid salt is present. An empty `remote` string is rejected at parse time.

## Validation Rules

@ref src/engine/config.rs#ValidationRule

Two rule shapes exist, discriminated by a `shape` tag in the TOML. A `parent-child` rule requires documents of one type to link to documents of another type via a named relation. A `relation-existence` rule requires documents of a given type to have at least one relation of any kind. Both shapes carry a `severity` field that is either `error` or `warning`.

@ref src/engine/config.rs#default_rules

When no `[[rules]]` section is present, three default rules apply: stories should implement rfcs (warning), iterations must implement stories (error), and adrs must have at least one relation (error). Providing any `[[rules]]` section replaces the defaults entirely.

## Naming and Filesystem

The `Naming` struct holds a single `pattern` string. The default is `{type}-{n:03}-{title}.md`, which produces filenames like `RFC-001-my-title.md`. The `FilesystemConfig` struct groups a `Directories` mapping (derived from type definitions when not explicitly set) and a `Templates` struct pointing at the template directory (defaulting to `.lazyspec/templates`).

## UI Configuration

@ref src/engine/config.rs#UiConfig

The `[tui]` section contains a single boolean, `ascii_diagrams`, which defaults to `false`. When enabled, the TUI renders diagrams using ASCII characters instead of Unicode box-drawing glyphs.
