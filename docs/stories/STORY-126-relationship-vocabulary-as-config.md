---
title: Relationship vocabulary as config
type: story
status: accepted
author: jkaloger
date: 2026-06-18
tags: []
related:
- implements: RFC-042
---

## Context

Relationship types are a closed Rust enum (`Implements`, `Supersedes`, `Blocks`, `RelatedTo`) with hardcoded names, inverse aliases, and keyword resolution. A project cannot introduce its own relationship vocabulary without changing engine code. Document types are already open via the `DocType(String)` newtype declared in `.lazyspec.toml`; this story brings relationships to parity by making `RelationType` a config-driven newtype whose vocabulary (name + optional inverse) is declared in a `[[relationships]]` block.

## Acceptance Criteria

- **Given** a `.lazyspec.toml` declaring a custom relationship set including a name absent from the old enum (e.g. `tracks`)
  **When** a user links one document to another using that custom name
  **Then** the link succeeds and the relationship is written to frontmatter under the configured name.

- **Given** a config whose `[[relationships]]` block does not declare the requested name
  **When** a user runs `link` with that unknown relationship name
  **Then** the command is rejected with a clear error identifying the unknown relationship, and no frontmatter is written.

- **Given** an existing document whose frontmatter carries a relationship name not present in `[[relationships]]`
  **When** a user runs `validate`
  **Then** validation flags that document with an error naming the unknown relationship.

- **Given** a relationship declared with an `inverse` name (e.g. `implements` / `implemented-by`)
  **When** a user links using the inverse name
  **Then** the relationship is stored once on the opposite document with the direction flipped, matching today's behaviour but with the inverse name sourced from config rather than hardcoded.

- **Given** a relationship declared with no `inverse` field
  **When** a user links with that relationship (e.g. `related-to`)
  **Then** it is treated as symmetric, with no separate inverse keyword accepted or required.

- **Given** a `.lazyspec.toml` with no `[[relationships]]` section at all
  **When** any command that loads config runs
  **Then** config loading fails with a hard error stating that `[[relationships]]` is required.

- **Given** any relationship linked under its configured name
  **When** a command is run with `--json`
  **Then** the relationship serializes under that configured name in the JSON output.

## Scope

### In Scope

- `[[relationships]]` config block parsed into a `RelationshipDef { name, inverse: Option<String> }` registry loaded onto `Config`.
- Replacing the closed `RelationType` enum with a `RelationType(String)` newtype mirroring `DocType`, including the ~89-site refactor across engine, CLI, and TUI.
- `link`/`unlink` consuming the registry: `link` rejects an unknown relationship name; inverse names resolved from the config `inverse` field instead of hardcoded `INVERSE_STRS` / `resolve_rel_keyword`.
- `validate` flagging documents that carry a relationship name not declared in config.
- Strict hard error when `[[relationships]]` is absent.
- `--json` output serializing relationships by configured name.

### Out of Scope

- Type-pair constraints — they remain in the existing `[[rules]]` block, untouched.
- Doc-type defaults and the Directories struct (STORY-125).
- `init` writing the `[[relationships]]` block scaffold (STORY-127).
- `fix` migration of existing configs/docs (STORY-128).
