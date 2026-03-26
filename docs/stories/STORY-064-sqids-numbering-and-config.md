---
title: Sqids numbering and config
type: story
status: accepted
author: agent
date: 2026-03-16
tags: []
related:
- implements: RFC-027
---




## Context

Document numbering currently uses sequential integers (`RFC-001`, `RFC-002`). This breaks in distributed workflows where two branches can independently claim the same number. Sqids provides short, unique, non-sequential IDs as an alternative strategy, configured per-type.

This story covers the core numbering infrastructure: the `sqids` dependency, config parsing, ID generation dispatch, and integration with the `create` command. ID resolution and migration are handled in separate stories.

## Acceptance Criteria

- **Given** a `.lazyspec.toml` with `numbering = "sqids"` on a type and a valid `[numbering.sqids]` section
  **When** `lazyspec create` is run for that type
  **Then** the generated document filename uses a sqids-encoded ID (e.g. `RFC-k3f-my-title.md`)

- **Given** a `.lazyspec.toml` with no `numbering` field on a type
  **When** `lazyspec create` is run for that type
  **Then** the document uses incremental numbering as before

- **Given** a `.lazyspec.toml` with `numbering = "incremental"` on a type
  **When** `lazyspec create` is run for that type
  **Then** the document uses incremental numbering as before

- **Given** a `[numbering.sqids]` section with a `salt` value
  **When** sqids IDs are generated
  **Then** the salt influences the output alphabet so IDs differ from the unsalted default

- **Given** a `[numbering.sqids]` section with `min_length = 5`
  **When** a sqids ID is generated
  **Then** the ID is at least 5 characters long

- **Given** a `[numbering.sqids]` section with `min_length` outside the range 1-10
  **When** the config is loaded
  **Then** validation fails with a clear error message

- **Given** `numbering = "sqids"` on a type but no `[numbering.sqids]` section with a `salt`
  **When** the config is loaded
  **Then** validation fails indicating that `salt` is required for sqids numbering

- **Given** a directory with existing sqids-numbered documents
  **When** `lazyspec create` generates the next ID
  **Then** the input integer is `count + 1` of existing documents, and if the resulting filename collides, the input increments until no collision occurs

- **Given** sqids numbering is configured
  **When** a document is created
  **Then** the ID portion of the filename is lowercase

## Scope

### In Scope

- Add `sqids` crate dependency
- `NumberingStrategy` enum (`Incremental`, `Sqids`)
- `SqidsConfig` struct (`min_length`, `alphabet`, `salt`)
- Parse `numbering` field on `[[types]]` entries
- Parse `[numbering.sqids]` global config section
- `next_id` function that dispatches to incremental or sqids generation
- Update `create` command to use `next_id`
- Validate: `salt` required when sqids is used, `min_length` in range 1-10

### Out of Scope

- ID resolution changes (`extract_id_from_name`, `resolve_shorthand`) -- Story B
- `fix --renumber` migration between strategies -- Story C
- Changes to `lazyspec validate` document-level validation
- Custom per-type sqids config (all sqids types share one global config)
