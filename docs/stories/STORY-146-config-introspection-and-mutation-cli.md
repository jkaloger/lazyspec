---
title: Config introspection and mutation CLI
type: story
status: draft
author: jkaloger
date: 2026-06-21
tags: []
related:
- implements: RFC-048
---

## Context

RFC-048 makes the workflow config-driven on the spine that the binary owns data and the skill owns prose. The skills need to read the whole DAG (types, relations, rules, gates) at runtime, and the `/configure-type` meta-skill needs to mutate `.lazyspec.toml` without hand-editing TOML. STORY-145 adds the config axes (`intent`, `authorship`, `lifecycle`, and the `require_parent_status` gate); this slice exposes them. It serves the full config as JSON and adds mutation subcommands that edit the file in place through the existing `config_write.rs`, preserving comments and formatting exactly as TUI settings and `fix --config` already do.

## Acceptance Criteria

- **Given** a `.lazyspec.toml` with types carrying `intent`, `authorship`, and `lifecycle`
  **When** `lazyspec config --json` runs
  **Then** the output lists every configured type with its `intent`, `authorship`, and `lifecycle` (states and edges) populated

- **Given** a config with relationships, validation rules, and a parent-child gate
  **When** `lazyspec config --json` runs
  **Then** the relations, rules, and gates appear in the JSON alongside the types

- **Given** an existing config
  **When** `lazyspec config add-type` adds a new type and `config --json` is read back
  **Then** the new type is present in the config with the supplied fields, and the round-trip preserves it

- **Given** a type whose lifecycle is the default DAG
  **When** `lazyspec config set-lifecycle` updates that type's states and edges
  **Then** `config --json` reports the type's lifecycle with the new states and edges

- **Given** a parent-child rule with no gate
  **When** `lazyspec config add-gate` sets a `require_parent_status` on it
  **Then** `config --json` reports that rule carrying the new `require_parent_status` value

- **Given** a `.lazyspec.toml` with comments and custom table ordering
  **When** any `config` mutation subcommand edits it
  **Then** the existing comments, formatting, and table order are preserved in the written file

## Scope

### In Scope

- `lazyspec config --json`: emit the full config as JSON — all types (including the `intent`/`authorship`/`lifecycle` axes), relationships, validation rules, and parent-status gates
- `lazyspec config add-type`: append a new type to `.lazyspec.toml`
- `lazyspec config set-lifecycle`: update a type's lifecycle states and edges
- `lazyspec config add-gate`: add a `require_parent_status` gate to a parent-child rule
- Mutation subcommands edit in place via the existing `config_write.rs`, preserving comments, formatting, and table order
- Help text for the new subcommands and README updates for the new CLI surface

### Out of Scope

- The config axes and schema themselves — `intent`, `authorship`, `lifecycle`, `require_parent_status` (STORY-145)
- The generic verb skills that consume `config --json` (STORY-147)
- Enriched templates and `init` materialization (STORY-148)
- The `/configure-type` meta-skill that drives the mutation CLI (STORY-149)
- A routing/eligibility `next`/`guide` command — deliberately none in v1 (RFC-048 non-goal)
