---
title: TOML config schema for LLM reference and LSP autocomplete
type: rfc
status: accepted
author: jkaloger
date: 2026-07-10
tags: []
related:
- related-to: RFC-013
- related-to: RFC-023
---

## Summary

Emit a JSON Schema for `.lazyspec.toml`, generated from the config structs, and serve it through a new `lazyspec config schema` command. One artifact feeds two consumers: TOML language servers (taplo / Even Better TOML) get editor autocomplete and inline validation, and LLM agents get an authoritative, machine-readable config reference instead of parsing README prose.

## Problem

The config schema exists only as README prose and as the private Rust structs in `src/engine/config.rs`. Two consequences:

1. **Humans edit `.lazyspec.toml` blind.** No autocomplete, no hover docs, no validation until lazyspec next parses the file. RFC-023 identified this directly: "Users need to know the config schema, find the file, open it in an editor" — its answer was a TUI settings screen, which helps interactive users but not editor-first ones.
2. **Agents have no programmatic schema.** The convention requires that convention content be retrievable programmatically and that agents consume the same interfaces humans do. Today an agent configuring a lazyspec project (e.g. the `configure-type` skill) must infer valid keys from README examples or `config --json` output — the latter shows the *current* config's values, not the *space* of valid config.

RFC-013 made the schema config-driven and therefore bigger: `[[types]]`, `[[rules]]`, lifecycles, gates. The surface area grew past what prose documentation reliably covers.

## Design Intent

### Schema source of truth: derive from the parse-path structs

Generate the schema with `schemars` (`#[derive(JsonSchema)]`) on the structs that actually deserialize `.lazyspec.toml`.

The critical wrinkle: `Config::parse_inner` deserializes the private `RawConfig` struct (`src/engine/config.rs:641`), then validates and assembles `Config` manually. `Config`'s own derives are used only for serialization. Deriving `JsonSchema` on `Config` would describe the *output* shape (with `#[serde(skip)]` fields, assembled defaults) and silently diverge from what the TOML file accepts. The schema MUST derive from `RawConfig` and its nested types — that is the real input grammar.

`schemars` honors the serde attributes already present, which is what makes derivation trustworthy here:

- `ValidationRule`'s internal tag `#[serde(tag = "shape")]` with kebab-case variant renames
- `StoreBackend`'s per-variant renames (`filesystem`, `github-issues`, `clickup-tasks`, …)
- `rename_all = "lowercase"` enums (`Severity`, `Traversal`, `NumberingStrategy`, `Authorship`, `AttrKind`, `ReservedFormat`)
- `#[serde(flatten)]`, `#[serde(default)]`, field renames like `github_label`

Dependency cost is near zero: `schemars` is already in the dependency graph transitively (via `serde_with` and tauri crates). Add it as a direct dependency at 1.x.

### Doc comments become hover docs

`schemars` lifts Rust doc comments into schema `description` fields. Part of this work is writing doc comments on every `RawConfig` field and nested config struct — these surface as editor hover text and as the LLM-readable field reference. The README's existing config sections (Custom Types, Lifecycle, Relationships, Validation Rules, Numbering, Templates, Agents) are the source material to condense into field-level comments.

### CLI: `lazyspec config schema`

New variant on the existing `ConfigCommand` enum (`src/cli/config.rs`, alongside `Show`, `AddType`, `SetLifecycle`, `AddGate`):

```
lazyspec config schema          # prints JSON Schema to stdout
lazyspec config schema --json   # identical; schema is already JSON
```

Prints the generated schema to stdout. No file paths, no project state required — the schema is a property of the binary, so the command works outside a lazyspec project. Engine exposes a `config_schema()` function returning `schemars::Schema`; CLI serializes it. TUI and web view are unaffected: the schema describes the config file format, which only the editor/agent surface touches. (RFC-023's settings screen could later consume the same schema to drive its field list, but that is not in scope.)

### Editor wiring: `#:schema` directive

Taplo (the LSP behind Even Better TOML) resolves a `#:schema <url-or-path>` directive at the top of a TOML file. `lazyspec init` writes this header into new `.lazyspec.toml` files, pointing at a published schema URL for the running version. Existing projects add the one-line header by hand or via `lazyspec init`'s migration path.

The schema is published per release as a GitHub release asset (e.g. `https://github.com/<org>/lazyspec/releases/download/v0.X.Y/lazyspec.schema.json`), produced in CI by running `lazyspec config schema`. This keeps the URL stable per version and requires no external registry.

### Agent wiring

Agents and skills that touch config (`configure-type`, `/lazy` routing) get one instruction change: read `lazyspec config schema` for the valid-config space, `lazyspec config --json` for current values. No new protocol.

## What this doesn't cover

- **SchemaStore submission** — worthwhile once the schema stabilizes (gives zero-config autocomplete, no `#:schema` header needed), but premature pre-1.0 while the config surface still moves. Revisit at 1.0.
- **Validating `.lazyspec.toml` against the schema at parse time** — `Config::parse_inner`'s hand-written validation stays authoritative; the schema is documentation/tooling, not a second validator. Semantic rules (e.g. lifecycle edges referencing declared states, `parent_type` referencing a declared type) can't be expressed in JSON Schema anyway.
- **RFC-023's settings screen consuming the schema** — natural follow-up, out of scope.

## Open Questions

1. **Schema versioning / URL stability.** Per-release asset URLs mean an old `#:schema` header points at an old schema. Acceptable (config compat tracks binary version), or should a `latest` alias exist?
2. **`RawConfig` visibility.** Deriving `JsonSchema` on private types is fine within the crate, but if the web view or another crate ever needs the schema type, `config_schema()` returning the built `Schema` object keeps `RawConfig` private. Confirm that boundary holds.

## Stories

1. **Derive and emit the schema** — add `schemars` 1.x, derive `JsonSchema` across `RawConfig` and nested config types, expose `config_schema()` in engine, add `lazyspec config schema` to the CLI, update README.
2. **Doc comments as descriptions** — write field-level doc comments across the config structs, condensed from README config sections; verify they appear as `description` in the emitted schema.
3. **Publish and wire up** — CI step emitting the schema as a release asset; `lazyspec init` writes the `#:schema` header; update agent-facing skills to reference `config schema`.

