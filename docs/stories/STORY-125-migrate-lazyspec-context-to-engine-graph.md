---
title: Migrate lazyspec context to engine Graph
type: story
status: draft
author: jkaloger
date: 2026-04-30
tags: []
related:
- implements: RFC-041
---


## Context

RFC-041 states explicitly: *"Both surfaces (this RFC and `lazyspec context`) share the same `engine::Graph` substrate; `context` walks `implements` only, sequencing walks both."* STORY-120 ships the new `engine::Graph` (single petgraph, typed `EdgeKind`, `Graph::from_store`). This story migrates the existing `lazyspec context` command onto that substrate so the codebase has a single canonical graph layer.

Today `cli/context.rs::resolve_chain` walks `implements` ancestors via `Store::get` per relation and uses `Store::forward_links` / `reverse_links` directly. After STORY-120 lands, that adjacency is reachable through `engine::Graph` filtered by `EdgeKind::Implements`. Leaving `cli/context.rs` on the bespoke walk would leave two parallel graph implementations, undermining RFC-041's "shared substrate" intent and Principle 6 (don't keep two indirections once one suffices).

This is also the right point to lift `resolve_chain`'s pure transformation logic out of the CLI layer and into engine, per Principle 3 (engine = core logic; CLI = dispatch + formatting).

## Acceptance Criteria

1. **Given** a document id and a populated `Store`,
   **When** `lazyspec context <id>` is invoked,
   **Then** the resolved chain (implements-ancestors), forward children (reverse-implements), and related (`RelatedTo`) collections match the pre-migration behaviour exactly.

2. **Given** a document id and a populated `Store`,
   **When** the new engine API for context resolution is called,
   **Then** it returns the same `ResolvedContext`-equivalent result that `cli/context.rs::resolve_chain` produced before migration.

3. **Given** the migration is complete,
   **When** the codebase is searched for direct uses of `Store::forward_links` / `Store::reverse_links` / `Store::children_of` / `Store::parent_of` from the CLI layer,
   **Then** none remain in `cli/context.rs`; all such access happens via `engine::Graph`.

4. **Given** the migration is complete,
   **When** the engine module structure is inspected,
   **Then** the context-resolution logic lives in engine (e.g. `engine::sequencing` or a sibling module), and `cli/context.rs` only formats the engine result for human/JSON output.

5. **Given** the migration is complete,
   **When** `cargo test` runs,
   **Then** all existing context-related tests pass without modification to their assertions (behaviour-preserving refactor).

6. **Given** the migration is complete,
   **When** `lazyspec context <id> --json` is invoked,
   **Then** the JSON output is byte-equivalent to the pre-migration JSON for the same document set (no schema drift).

## Scope

### In Scope

- A new engine API for "resolve context chain for a document" backed by `engine::Graph`. Walks `Implements` edges for ancestors and reverse-`Implements` for forward children. Walks `RelatedTo` edges for the `related` collection.
- Migrating `cli/context.rs::resolve_chain` to call the new engine API. CLI retains formatting (`run_human`, `run_json`, `mini_card`) but no graph logic.
- Removing direct `Store::forward_links` / `reverse_links` / `children_of` / `parent_of` calls from `cli/context.rs`.
- Behavioural-equivalence tests that compare new vs old output across a fixture set.

### Out of Scope

- Changes to `lazyspec context` CLI surface, flags, or output schema.
- Retiring `Store::forward_links` / `reverse_links` themselves (still used by `validation` and other call sites). Engine `Graph` borrows them; it does not replace them in this story.
- New context-related features (e.g. cross-RFC navigation). Pure refactor.
- Removal or reshape of any `Store` API.
