---
title: Config-driven type definitions
type: story
status: accepted
author: jkaloger
date: 2026-03-06
tags: []
related:
  - implements: docs/rfcs/RFC-013-custom-document-types.md
---

## Context

Document types are hardcoded as a Rust enum (`DocType`) with four variants. The `Config::Directories` struct has one named field per type. This makes it impossible to add or replace types without code changes. RFC-013 introduces a `[[types]]` config array so users can define their own type set.

## Acceptance Criteria

- **Given** a `.lazyspec.toml` with no `[[types]]` section
  **When** the config is loaded
  **Then** the default types (rfc, adr, story, iteration) are available with their current directories, plurals, and prefixes

- **Given** a `.lazyspec.toml` with a `[[types]]` section defining custom types
  **When** the config is loaded
  **Then** only the custom types are available (defaults are replaced, not merged)

- **Given** a `[[types]]` entry with `name`, `plural`, `dir`, and `prefix` fields
  **When** the config is parsed
  **Then** a `TypeDef` is created with all four fields populated

- **Given** a `[[types]]` entry missing a required field
  **When** the config is parsed
  **Then** a clear error is returned indicating which field is missing

- **Given** the `DocType` enum is replaced with a `DocType(String)` newtype
  **When** a document's frontmatter contains `type: rfc` (or any configured type name)
  **Then** it deserializes into the corresponding `DocType` value

- **Given** a document with a `type:` value that doesn't match any configured type
  **When** the document is parsed
  **Then** an error is returned listing the valid type names

## Scope

### In Scope

- `[[types]]` config parsing with `name`, `plural`, `dir`, `prefix`
- `DocType` enum to `DocType(String)` newtype conversion
- `Config::Directories` struct replaced with `Vec<TypeDef>`
- Default type definitions matching current behavior
- Deserialization of `type:` frontmatter field against configured types

### Out of Scope

- Validation rules (covered by STORY-038)
- CLI command updates (covered by STORY-039)
- Custom statuses or relation types
- Migration tooling for existing `.lazyspec.toml` files
