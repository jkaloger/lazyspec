---
title: init writes relationships and rules
type: story
status: accepted
author: jkaloger
date: 2026-06-18
tags: []
related:
- implements: RFC-042
---

## Context

ADR-011 strips the engine of all built-in defaults: relationship vocabulary and validation rules no longer exist in code, so a loaded project must declare them in `.lazyspec.toml` or it won't load. That makes `init` the sole source of the starter set. Today `init` already writes the `[[types]]` block; it must now also emit the 4 historical relationships and the 3 historical rules so a fresh checkout loads and behaves identically to the pre-refactor builtins.

## Acceptance Criteria

- **Given** an empty directory with no `.lazyspec.toml`
  **When** `lazyspec init` runs
  **Then** the generated `.lazyspec.toml` contains a `[[relationships]]` block declaring the 4 historical relationships: `implements` (inverse `implemented-by`), `supersedes` (inverse `superseded-by`), `blocks` (inverse `blocked-by`), and `related-to`.

- **Given** the same `init` run
  **When** the `related-to` relationship is examined
  **Then** it is written as symmetric with no `inverse` field.

- **Given** an empty directory with no `.lazyspec.toml`
  **When** `lazyspec init` runs
  **Then** the generated `.lazyspec.toml` contains the 3 historical `[[rules]]`: `stories-need-rfcs` (parent-child story→rfc, link `implements`, severity warning), `iterations-need-stories` (parent-child iteration→story, link `implements`, severity error), and `adrs-need-relations` (relation-existence on adr, require `any-relation`, severity error).

- **Given** a project freshly created by `init`
  **When** any command that loads the config runs against it
  **Then** the config loads with no strict-load error, and validation produces the same results as the pre-refactor engine builtins.

- **Given** a directory where `.lazyspec.toml` already exists
  **When** `lazyspec init` runs
  **Then** it refuses with the existing "already exists" error and writes nothing (unchanged behavior; migrating existing configs is out of scope).

## Scope

### In Scope

- `lazyspec init`'s scaffold output emits a `[[relationships]]` block with the 4 historical relationships and their inverses (`related-to` symmetric, no inverse), alongside the `[[types]]` it already writes.
- `lazyspec init`'s scaffold output emits a `[[rules]]` block with the 3 historical rules.
- A freshly `init`-ed project loads cleanly under strict load and validates identically to the pre-refactor builtins.

### Out of Scope

- Migrating existing `.lazyspec.toml` files via `fix` (STORY-128).
- The relationship registry / newtype machinery (STORY-126).
- Removing the defaults from the engine (STORY-125).
