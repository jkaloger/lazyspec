---
title: Emit JSON Schema for .lazyspec.toml via config schema command
type: story
status: in-progress
author: jkaloger
date: 2026-07-10
tags: []
related:
- implements: RFC-058
---

## Story

As a developer or agent editing `.lazyspec.toml`, I want `lazyspec config schema` to emit an authoritative JSON Schema of the config format, so I can write valid config with editor autocomplete and machine-readable reference instead of inferring from README prose.

Walking skeleton for RFC-058: command + derived schema + field descriptions. Publishing (CI release asset, `#:schema` header in `init`) is a follow-up slice.

## Acceptance Criteria

1. **Emit anywhere.** Given any directory (lazyspec project or not), when `lazyspec config schema` runs, then stdout is one valid JSON Schema document, exit 0. `--json` accepted; output identical.
2. **Faithful to parse path.** Schema derives from `RawConfig` and nested types (the structs `Config::parse_inner` deserializes) — not from `Config`. Given this repo's own `.lazyspec.toml` converted to JSON, when validated against the emitted schema, then it passes.
3. **Serde fidelity.** Given TOML with `severity = "fatal"` (invalid enum value), when validated against the schema, then validation fails. Rule variants keyed by `shape` tag; store backends use kebab-case names (`github-issues`, `clickup-tasks`); lowercase enum renames honored.
4. **Descriptions present.** Every top-level config section and every `[[types]]` field carries a non-empty `description` sourced from Rust doc comments.
5. **Layering.** Schema generation lives in engine (`config_schema()` returning `schemars::Schema`); CLI only serializes. `RawConfig` stays private.
6. **README documents the command** in the config inspection section.

## Non-Goals

- Publishing schema per release; `lazyspec init` writing `#:schema` header (follow-up story)
- SchemaStore submission
- Runtime validation of config against the schema — `parse_inner`'s hand-written validation stays authoritative

