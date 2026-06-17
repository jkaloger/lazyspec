---
title: Extract context resolution into engine
type: iteration
status: draft
author: agent
date: 2026-06-18
tags:
- engine
- context
- refactor
related:
- related-to: STORY-123
- related-to: STORY-124
- related-to: SPEC-012
- implements: STORY-122
- blocks: ITERATION-175
- blocks: ITERATION-176
---

## Goal

Lift the context-graph traversal out of the CLI layer into the engine so all three
surfaces — the `lazyspec context` command, the TUI graph view, and the TUI relations
tab — can consume one shared implementation. Convention principle 3 puts core logic in
the engine (CLI and TUI depend on engine, never on each other); principle 6 is satisfied
because there are three concrete uses.

This iteration is behaviour-preserving for the CLI with one deliberate exception: the
shorthand-resolution fix below, which makes a parent RFC appear in a story's chain where
it is currently dropped. The TUI surfaces are not touched here (STORY-123, STORY-124
depend on this iteration).

## Changes

1. **Create `src/engine/context.rs` and register it.** (foundation for tasks 2-5)
   - Add `pub mod context;` to `src/engine.rs` (alphabetical position, after `config`).
   - Move these items verbatim from `src/cli/context.rs` into the new engine module:
     `ContextNode`, `RelatedRef`, `ResolvedContext`, `resolve_chain`, and `topo_order`.
   - Imports become engine-internal: `use crate::engine::document::{DocMeta, RelationType};`
     and `use crate::engine::store::{ResolveError, Store};`. The module must have no I/O
     and no formatting — it returns data structures only.
   - Leave all rendering and JSON serialization (`run_human`, `run_json`, `mini_card`,
     `chain_connector`, `render_stack`, `render_tree`, `render_tree_node`,
     `push_card_children`) in `src/cli/context.rs`.
   - Verification: `cargo build` succeeds; `src/engine/context.rs` imports nothing from
     `crate::cli`.

2. **Fix the shorthand-resolution gap in the upward `implements` walk.** (the one
   intentional behaviour change)
   - In `resolve_chain`, the upward walk currently resolves each parent with
     `store.get(&PathBuf::from(&rel.target))` (was `src/cli/context.rs:60`). When
     `rel.target` is a shorthand such as `"RFC-006"`, `PathBuf::from` yields a non-path
     and `store.get` returns `None`, so the ancestor is dropped. Root cause confirmed
     against `rebuild_links`/`build_links` in `src/engine/store/links.rs`, which resolve
     targets via `resolve_target(&rel.target, &id_to_path)` — so the link maps hold
     resolved paths but the upward walk does not use them.
   - Add a public resolver on `Store` (in `src/engine/store.rs`, near `resolve_shorthand`
     at line 165): `pub fn resolve_relation_target(&self, target: &str) -> Option<&DocMeta>`.
     It looks the target up as a document id first (id → path), then falls back to
     treating it as a path, mirroring the private `resolve_target` logic in
     `src/engine/store/links.rs:108`. Return the `&DocMeta` via `self.get`.
   - In the engine `resolve_chain` upward walk, replace
     `store.get(&PathBuf::from(&rel.target))` with `store.resolve_relation_target(&rel.target)`.
   - Do NOT switch the walk to `store.forward_links_for(...)`: `propagate_parent_links`
     (`src/engine/store/links.rs:30`) copies a parent's forward links onto nested child
     docs, so a nested doc's `forward_links` include inherited `implements` edges. The
     walk must stay on the doc's own declared `current.related` to avoid over-collecting
     ancestors for nested docs.
   - Verification: `context STORY-122 --json` now shows RFC-007 in `chain`;
     `context STORY-123 --json` shows RFC-006 in `chain`.

3. **Re-point the CLI to the engine module, behaviour-preserving.**
   - In `src/cli/context.rs`, replace the moved definitions with
     `use crate::engine::context::{ResolvedContext, RelatedRef, resolve_chain};`
     (and `ContextNode` if referenced by the renderers — `render_tree` uses it).
   - `run_human` and `run_json` keep calling `resolve_chain(store, id, depth)` exactly as
     before; only the import path changes. No output format changes.
   - Verification: existing CLI context integration tests pass unchanged except for the
     shorthand-fix assertions added in the test plan; `cargo build` and `cargo clippy`
     clean.

4. **Add `resolve_forest` to the engine module.** (consumed later by STORY-123; landed
   here so the engine API is complete and tested at the seam)
   - Signature: `pub fn resolve_forest(store: &Store) -> Vec<ContextNode<'_>>` (or a
     thin `ResolvedForest` wrapper if a forest-level field is needed; a `Vec<ContextNode>`
     in deterministic order is sufficient for the graph view's needs).
   - Semantics: whole-store, roots-first. Roots are documents whose own `related` contains
     no `implements` entry (matching `src/tui/state/app.rs#rebuild_graph` at line 621).
     Build the full DAG by discovering every document and its in-graph `implements`
     parents (resolved via `resolve_relation_target`), then order with the existing
     `topo_order` so multi-parent nodes appear after all their parents and each node
     appears exactly once. Cycle-safe via the same seen-set / leftover handling as
     `resolve_chain`.
   - This must preserve multi-parent edges (each `ContextNode.parents` lists all in-graph
     parents) — that is the divergence STORY-123 fixes in the graph view.
   - `resolve_forest` is exercised by unit tests here but not yet wired into the TUI
     (out of scope; STORY-123).
   - Verification: unit tests in task-5 cover single-root, multi-root, diamond, and cycle.

5. **Engine-level unit tests for the resolution module.**
   - Add `#[cfg(test)]` unit tests in `src/engine/context.rs` for `resolve_chain` and
     `resolve_forest` over an in-memory `Store` (construct via the same fixtures other
     `src/engine/store.rs` tests use). Cover: linear chain order, diamond dedup,
     multi-parent edge retention, cycle termination, depth-N related BFS shortest-hop
     `distance`, and shorthand-target parent resolution.

## Test Plan

The behaviour-preservation guarantee rests on the existing CLI context integration tests
as a characterization suite. They assert the current human and `--json` output; if the
extraction is faithful they pass without modification.

- **Behaviour preservation (CLI human + JSON), AC: tasks 1, 3.**
  Run the existing suites unchanged: `tests/integration/cli_context_test.rs`,
  `tests/integration/cli_child_context_test.rs`,
  `tests/integration/cli_git_ref_context_test.rs`. These are behavioural and specific
  (assert rendered output and JSON keys/membership). No new assertions; passing them is
  the safety net. If any output legitimately must change, that is a regression and the
  extraction is wrong — except the shorthand case below.

- **Shorthand-resolution regression test, AC: task 2.**
  New integration test in `tests/integration/cli_context_test.rs`: a fixture where a
  story `implements` an RFC by shorthand id (e.g. `implements: RFC-006`). Assert that
  `context <story> --json` includes the RFC's path in `chain` and that `target` is the
  story. This is the one intentional behaviour change; the test documents it. Property
  tradeoff: deterministic and behavioural (asserts membership in `chain`, not byte
  output) so it is robust to ordering of unrelated fields.

- **Engine unit tests, AC: tasks 4, 5.**
  In-module tests on `resolve_chain`/`resolve_forest` over a hand-built `Store`. These
  are isolated (no filesystem, no CLI), deterministic (path-sorted ordering is already
  enforced in `topo_order` and the related BFS), and specific (assert node sets, edge
  sets per node, and `distance`/`via` tags). Forest tests assert multi-parent edges are
  retained and cycles terminate with each node present once.

- **Layering check, AC: task 1.**
  Confirm `src/engine/context.rs` has no `use crate::cli` import (grep). Enforces
  principle 3 (engine has no CLI dependency).

## Notes

- Discovered during planning: there are three divergent traversals of the same
  relationship graph — `src/cli/context.rs#resolve_chain` (DAG, this branch),
  `src/tui/state/graph.rs#traverse_dependency_chain` (forest, single-edge, no
  related-to), and the `implements` chain walk in
  `src/tui/views/panels.rs#render_relationship_sections` (single-parent via `find_map`).
  This iteration creates the single engine implementation the TUI stories then adopt.
- The forest-vs-neighbourhood asymmetry is the engine API's design crux: the graph view
  needs a whole-store forest (`resolve_forest`); the CLI and relations tab need a
  single-target neighbourhood (`resolve_chain`). Both compose from the same primitives
  (upward DAG walk + `topo_order` + resolved-target lookup), which is why they belong in
  one module.
- `propagate_parent_links` is the trap in task 2: do not source the implements walk from
  `forward_links_for`, or nested docs inherit their parent's ancestors. Walk declared
  `related` and resolve the target id.
- SPEC-012 already documents `resolve_chain` and its data structures with `@ref` to
  `src/cli/context.rs`. After this move those refs point at the wrong module; updating
  SPEC-012's `@ref` paths to `src/engine/context.rs` is a follow-up (out of scope for the
  code change, but flag it so the spec does not drift).
