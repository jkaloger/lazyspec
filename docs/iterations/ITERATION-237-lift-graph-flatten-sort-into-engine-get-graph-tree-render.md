---
title: Lift graph flatten/sort into engine; GET /graph tree render
type: iteration
status: complete
author: unknown
date: 2026-06-30
tags: []
related:
- implements: STORY-179
---

## Objective

Lift the graph ordering logic (`flatten_forest`, `compare_siblings`, and the types they touch -- `GraphNode`, `GraphSort`, `SortKey`) out of `tui/` into `engine::graph` so TUI and web share one ordering implementation, then add `GET /graph` rendering the ordered nodes as a topologically-sorted nested `<ul>` tree.

## Satisfies

STORY-179, all ACs: the type/function lift with no `tui` dependency, the behavior-preserving TUI re-point, the relocated ordering tests, diamond (no duplicate-subtree re-emission) and cycle (terminates, each node once, back-edge dropped) handling, and `GET /graph` rendering with `GraphSort::default()`.

## Context

- Story + ACs (authoritative): STORY-179.
- RFC sections: RFC-052 §Routes (the `/graph` tree mirrors the TUI), §Layering (web -> engine only, never tui).
- Convention principle 6: the lift is justified because both TUI and web are concrete consumers of one ordering implementation.
- Already in engine, reused unchanged: `resolve_forest` / `topo_order` in `src/engine/context.rs`.
- Source of the moved logic: `flatten_forest` / `compare_siblings` / `GraphSort` / `SortKey` in `src/tui/state/graph.rs`; `GraphNode` in `src/tui/state/app.rs`. The existing graph ordering unit tests move with the functions.
- Touch: `src/engine/graph.rs` (new, the moved types/functions/tests), `src/engine.rs` (module wiring), `src/tui/state/graph.rs` + `src/tui/state/app.rs` (re-point at the moved code), `src/web/routes.rs` + `src/web/server.rs` (`GET /graph` handler + route), `src/web/render.rs` (tree view model), `templates/` (nested-`<ul>` tree templates).

## Tasks

1. Create `engine::graph` and MOVE `flatten_forest`, `compare_siblings`, `GraphNode`, `GraphSort`, `SortKey` into it with no `tui` dependency; `engine::graph` compiles without the `tui` module.
2. Relocate the existing graph ordering tests alongside the functions; they run against `engine::graph` and pass unchanged.
3. Re-point the TUI at the moved types/functions (pure refactor, no change to rendered ordering); keep the TUI graph tests green.
4. Add `GET /graph`: walk `resolve_forest` -> `flatten_forest(.., &GraphSort::default())` into a nested `<ul>` tree; diamonds draw the shared node once (plain row on the second branch, subtree not re-emitted); cycles terminate via the existing ordering logic.
5. Test-first the web route (oneshot, no real socket): default-sort nested tree, a diamond fixture, a cycle fixture.

## Out of scope

- Any change to graph ordering semantics (Kahn, diamond/cycle handling, sibling comparison stay identical).
- `resolve_forest` / `topo_order` -- already in engine, reused unchanged.
- Interactive sort selection on the web `/graph` view (default sort only; interactive sort stays a TUI concern).
- Mermaid/diagram rendering of the graph (plain nested lists).

## Principles / conventions

- RFC-052 layering principle 3: the web graph handler reaches ordering logic through `engine::graph`, never `tui`. New web code stays behind the `web` cargo feature.
- The lift is behavior-preserving: the TUI's rendered ordering is unchanged, verified by the relocated ordering tests and the existing TUI graph tests.

## Verification

- `grep crate::tui src/engine/graph.rs` is empty; `engine::graph` compiles in a default (non-web) build.
- The relocated unit tests pass unchanged under `engine::graph`; the TUI graph tests stay green.
- `GET /graph` renders a nested `<ul>` tree in `GraphSort::default()` order; a diamond's shared node is not re-emitted as a subtree; a cyclic component terminates with each node rendered once.
