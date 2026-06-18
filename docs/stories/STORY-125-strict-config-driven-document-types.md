---
title: Strict config-driven document types
type: story
status: draft
author: jkaloger
date: 2026-06-18
tags: []
related:
- implements: RFC-042
---

## Context

The engine bakes the RFC/Story/Iteration/ADR ontology into code via a hardcoded type fallback and a named-field `Directories` struct, even though doc types are already an open `DocType(String)` newtype that accepts any string. The opinionation lives only in the config fallback (`default_types()`), the `Directories` struct that structurally assumes four named types exist, and the `story`->`stories` pluralization helper. This slice removes that built-in vocabulary so the declared `[[types]]` in `.lazyspec.toml` is the sole source of truth, deriving directories from the types themselves. It is mostly deletion of the opinionated fallback and the `Directories` struct, not a model change.

## Acceptance Criteria

- **Given** a directory with no `.lazyspec.toml` present
  **When** any document command is run
  **Then** it fails with a clear error that points the user at `init`, rather than silently operating on a built-in type set.

- **Given** a `.lazyspec.toml` that exists but declares no `[[types]]` entries
  **When** the config is loaded
  **Then** loading is a hard error reporting the missing `[[types]]`, with no fallback to a default type set.

- **Given** a `.lazyspec.toml` declaring an arbitrary type set with no `rfc`, `story`, `iteration`, or `adr` types (e.g. only `ticket` and `epic`)
  **When** documents of those types are created, listed, and validated
  **Then** all operations succeed, proving the engine no longer assumes any specific type names exist.

- **Given** a `.lazyspec.toml` whose `[[types]]` declare `dir` values
  **When** the engine resolves where documents of each type live
  **Then** directories derive entirely from the declared `types` and their `dir`/`plural` fields, with no reliance on named `rfcs`/`adrs`/`stories`/`iterations` config fields.

- **Given** a `.lazyspec.toml` whose `[[types]]` omit a type that used to be built-in (e.g. `spec`)
  **When** the config is loaded and documents are listed
  **Then** that type is absent and is never injected by the engine, so referencing it is unknown.

- **Given** a `.lazyspec.toml` declaring a type named `story` with an explicit `plural` of `stories`
  **When** the config is loaded
  **Then** the plural is taken verbatim from the declared field, with no engine-side `story`->`stories` special-casing applied.

## Scope

### In Scope

- Remove the hardcoded built-in document type fallback so the engine carries no default types.
- Delete the named-field `Directories` struct and derive document directories from the declared `types` (and their `dir`/`plural` fields) instead.
- Make config loading a hard error when `[[types]]` is absent, with no silent default set.
- Remove the engine-side `story`->`stories` pluralization helper, relying on the required `plural` field on each declared type.

### Out of Scope

- Relationship model, `RelationType` newtype, registry, and the relationship-name refactor (STORY-126).
- `init` writing the `[[relationships]]`/`[[rules]]` blocks (STORY-127).
- `fix --config` migration and lenient read for existing projects (STORY-128).
