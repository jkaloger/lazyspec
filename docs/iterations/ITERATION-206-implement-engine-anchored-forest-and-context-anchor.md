---
title: Implement engine anchored forest and context --anchor
type: iteration
status: draft
author: jkaloger
date: 2026-06-23
tags: []
related:
- implements: STORY-151
---<!-- intent: plan the concrete changes that satisfy a story's acceptance criteria -->

## Changes

- `src/engine/context.rs` `resolve_forest` (`:171`): signature -> `resolve_forest(store, anchor: Option<&str>)`.
  - `None` -> current behaviour (all DAG roots).
  - `Some(type)` -> roots = docs where `doc.doc_type == type`; emit each + `implements`-descendant subtree only. Prune ancestors.
  - Reuse `topo_order` (`:203`) + seen-set guard for diamonds/cycles.
  - Descendant walk: forward `implements` (docs whose `related` implements the node), same edge logic as `:180`.
- Callers of `resolve_forest`: update to pass `None` (TUI `rebuild_graph` in `app.rs` for now; ITERATION-208 wires anchor).
- `src/cli/context.rs` (`:12` `run_json`) + `src/main.rs` (`:134` `Commands::Context`): add `--anchor <type>` arg. When set, call anchored forest path; emit JSON forest.
- README: document `context --anchor`.

## Test Plan

- AC1: anchor=story -> roots all stories, each w/ iteration descendants, no parent rfc. unit on fixture forest.
- AC2: `resolve_forest(store, None)` == today's output. regression unit.
- AC3: `context --json --anchor story` emits anchored; no flag emits whole-store. cli integration.
- AC4: doc w/ 2 anchor-type ancestors appears under each, no infinite loop. unit (diamond fixture).

## Notes

- Lineage stays `implements` hardcoded — no relation-param (RFC non-goal).
- `ContextNode` (`:7`) unchanged; only root set + pruning differ.
- Engine-only re-rooting (dictum 3); TUI consumes via ITERATION-208.
