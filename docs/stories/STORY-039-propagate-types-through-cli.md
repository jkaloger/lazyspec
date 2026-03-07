---
title: Propagate types through CLI and TUI
type: story
status: accepted
author: jkaloger
date: 2026-03-06
tags: []
related:
- implements: docs/rfcs/RFC-013-custom-document-types.md
---



## Context

With type definitions and validation rules now config-driven (STORY-037, STORY-038), every consumer of `DocType` needs updating. CLI commands like `create`, `init`, and the store loader match on enum variants and read from named directory fields. The TUI has a hardcoded type list (`app.rs:239`) and hardcoded graph icons per type (`ui.rs:905-910`). All of this needs to work against the dynamic `Config::types` list instead.

## Acceptance Criteria

- **Given** custom types are configured
  **When** `lazyspec create <custom-type> "title"` is run
  **Then** the document is created in the configured directory with the configured prefix

- **Given** custom types are configured
  **When** `lazyspec create <invalid-type> "title"` is run
  **Then** an error is returned listing the valid type names from config

- **Given** custom types are configured
  **When** `lazyspec init` is run
  **Then** directories are created for each configured type (not the hardcoded four)

- **Given** custom types are configured
  **When** the store loads documents
  **Then** it scans all configured type directories (not the hardcoded four)

- **Given** a custom type with a configured prefix
  **When** a document is created
  **Then** the filename uses the configured prefix (e.g. `EPIC-001-my-epic.md`)

- **Given** custom types are configured
  **When** the TUI launches
  **Then** the type tab bar shows all configured types (not the hardcoded four)

- **Given** a custom type in graph mode
  **When** the graph renders
  **Then** the node displays an icon (cycling through a default glyph set for types without a configured icon)

- **Given** default config (no `[[types]]`)
  **When** any CLI command or TUI view is used
  **Then** behavior is identical to the current implementation

## Scope

### In Scope

- `create.rs`: look up `TypeDef` by name instead of matching enum
- `init.rs`: iterate `config.types` for directory creation
- `store.rs`: iterate `config.types` for document scanning
- `template.rs`: use `TypeDef.prefix` for filename generation
- `tui/app.rs`: populate `doc_types` from config instead of hardcoded vec
- `tui/ui.rs`: replace hardcoded graph icon match with config-aware lookup
- Error messages with valid type hints

### Out of Scope

- Type definition parsing (covered by STORY-037)
- Validation rule evaluation (covered by STORY-038)
- New CLI commands
