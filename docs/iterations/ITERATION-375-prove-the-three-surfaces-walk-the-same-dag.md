---
title: Prove the three surfaces walk the same DAG
type: iteration
status: in-progress
author: jack
date: 2026-08-31
tags: []
related:
- implements: STORY-257
---

## Objective

`context --json`, the TUI graph and relations tab, and the web view render the same chain and neighbourhood for one document under one edge-table config -- proven by test, not by inspection -- and the before/after comparison STORY-258's migration must satisfy exists.

## Satisfies

STORY-257 AC6, and AC5 as far as this story can carry it (see below). AC1, AC2, AC3, AC4 landed in the preceding iterations; this closes the story.

**AC5 is mis-homed.** It reads "given any config migrated by `fix --config`, `context` output is identical before and after". `fix --config` migrates nothing until STORY-258, which owns the migration and its own behaviour-preservation criterion. STORY-257 can supply the instrument, not the result: this slice lands the golden `context --json` comparison, and STORY-258 asserts against it. Raise this with whoever grooms STORY-258 rather than reading AC5 as satisfied here.

## Context

- Story + ACs: STORY-257. Surface parity is an acceptance criterion, not a follow-up: STORY-257 §Notes
- Project instruction (`CLAUDE.md`): an engine change must account for the TUI, the web view and the CLI
- Touch:
  - `src/cli/context.rs` -- `resolve_chain` + `merge_declared_related`
  - `src/tui/state/app.rs` -- `relation_sections` (the same two calls) and the graph forest anchor selection
  - `src/web/routes.rs` -- `resolve_chain(.., 1)` **without** `merge_declared_related`, plus `resolve_forest`/`resolve_forest_by_tag`; `src/web/render.rs` layers the document's own `related` instead
  - `README.md`
- The web view's `serve` subcommand is behind the `web` feature, so its tests need `--features web`. A parity test that only ever runs under the default feature set proves nothing about the surface it names.

All three surfaces already call the same engine functions, so after the preceding slices AC6 is largely true by construction. The honest work is proving it and naming the one place it is not: the web view deliberately skips `merge_declared_related`. Whether that still produces the same set is the question this slice answers, not one it assumes.

## Tasks

1. Test-first: one fixture store and one config carrying per-edge traversal; assert the chain-ancestor id set and the related id set are equal across `cli::context`'s resolution, `AppState::relation_sections`, and the view model `src/web/render.rs` builds from `web::routes`' page context.
2. Resolve the web divergence one way or the other. Either the two paths demonstrably produce the same set -- assert it and record in a comment why both are kept -- or they do not, and the web view moves onto the shared call. Do not loosen `merge_declared_related`'s contract to force agreement; its exclusion of chain-typed relations is load-bearing.
3. Assert graph parity: for the fixture, `flatten_forest(resolve_forest(..))`'s parent edges for a document agree with `resolve_chain`'s parents for the same document, so the TUI graph view and the web `/graph` route cannot drift from `context`.
4. Land the AC5 instrument: a pair of configs -- one declaring traversal on `[[relationships]]`, one declaring the equivalent `[[edges]]` rows by hand -- over one fixture store, with the assertion that `context` output is identical under both. Name it so STORY-258 can find it; that story's migration is exactly the transformation between the two configs.
5. README: the chain and neighbourhood the CLI, TUI and web view render all come from one engine walk driven by the edge table. Remove any remaining claim that traversal is a property of `[[relationships]]` alone, keeping the fallback described in the preceding slices.

## Out of scope

- `fix --config` itself -> STORY-258. Retiring `[[rules]]` and `RelationshipDef.traversal` -> STORY-259.
- Editing edges from the TUI settings panel or the config CLI -> STORY-260, STORY-261.
- New rendering. No surface gains a control, column or route here; this slice proves the three agree and changes nothing a user would see on a config that has not declared edges.

## Principles / conventions

`lazyspec convention` and the dictums it lists. Dictum 3: parity is achieved by the three surfaces sharing one engine walk, never by three implementations kept in step.

## Verification

For one document in this repo, `cargo run -- context <id> --json`, the TUI Relations tab, and the document page under `cargo run --features web -- serve` list the same ancestors and the same related documents.
