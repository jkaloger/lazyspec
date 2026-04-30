---
title: Graph foundation cycle topo terminal-status critical-path
type: iteration
status: complete
author: agent
date: 2026-04-30
tags: []
related:
- implements: STORY-120
---



## Summary

Pure engine graph layer. Nodes from docs, typed `blocks`/`implements` edges. Cycle detect, topo, type-aware terminal, weighted critical-path. No I/O, no CLI, no TUI.

## Acceptance Criteria covered

- AC1: graph from docs preserves all nodes, both edge kinds queryable independently.
- AC2: cycle check on acyclic `blocks` returns success.
- AC3: cycle check on cyclic `blocks` returns failure with offending node ids.
- AC4: topo order respects `blocks` ordering on acyclic graph.
- AC9: critical-path with external weights returns max-cumulative-weight path honouring `blocks` ordering.
- AC10: terminal sets — RFC/Story `{complete, superseded, rejected}`; Iteration/Audit `{complete}`; ADR/Convention/Dictum `{accepted, superseded}`.
- AC11: `accepted` on non-decision type NOT terminal.

## Test Plan

All unit tests in `src/engine/sequencing.rs` `#[cfg(test)] mod tests`. Fixtures: in-memory `Vec<DocMeta>` built by helper. Deterministic. One assertion per behavior.

- AC1: arrange 3 docs w/ mix of `blocks` + `implements` edges, build graph, assert node count + each edge kind retrievable via separate accessors w/ correct endpoints. Separate query per kind = independence proof.
- AC2: arrange linear `blocks` chain A->B->C, run cycle_check, assert `Ok(())`.
- AC3: arrange `blocks` cycle A->B->C->A, run cycle_check, assert `Err(CycleError)` containing `{A,B,C}` ids (set equality, ignore order).
- AC4: arrange A->B, B->C, C->D `blocks`, run topo_order, assert position(A)<position(B)<position(C)<position(D). Use index lookup per pair, not full vec equality (multiple valid topos).
- AC9: arrange diamond A->B, A->C, B->D, C->D w/ weights {A:1,B:5,C:2,D:1}, assert critical_path returns [A,B,D]. Second case: equal weights tie-break stable + path still respects edges.
- AC10: parametric — for each (type, status) pair in spec, build single doc, assert is_terminal matches expected. Cover RFC@complete=true, RFC@accepted=false, Story@rejected=true, Iteration@superseded=false, Iteration@complete=true, ADR@accepted=true, ADR@complete=false, Convention@superseded=true, Dictum@accepted=true, Audit@complete=true, Audit@accepted=false.
- AC11: Story@accepted=false, Iteration@accepted=false (regression — `accepted` leaks only into decision types).

## Changes

Tasks self-contained for zero-context subagent. Each lists ACs, files, high-level intent, verification.

### 1. Add petgraph dep
- ACs: foundation for all.
- File: `Cargo.toml`.
- Add `petgraph = "0.6"` (or current stable; subagent picks latest stable patch) under `[dependencies]`.
- Verify: `cargo build`.

### 2. Scaffold sequencing module
- ACs: 1 (types only).
- New file: `src/engine/sequencing.rs`.
- Register in `src/engine.rs` w/ `pub mod sequencing;`.
- Define pub types:
  - `Graph` wrapping a single `petgraph::Graph<NodeRef, EdgeKind>` (directed). Node table shared.
  - `EdgeKind` enum `{ Blocks, Implements }` as edge weight.
  - `NodeRef` (newtype over `petgraph::graph::NodeIndex` or doc id string — pick whichever round-trips cleanly).
  - `Scope` (`All` | `Under(String)` | `After(String)`).
  - `Weights` (newtype `HashMap<String, f64>` keyed by doc id).
  - `CycleError` (carries `Vec<String>` offending ids).
- Two constructors:
  - `pub fn Graph::from_store(store: &Store) -> Self` — builds petgraph from `Store`'s existing `forward_links_for` / `reverse_links_for` typed adjacency. Production path. No re-parsing of `DocMeta.related`. Reuses Store as canonical source; no duplicate adjacency.
  - `pub fn Graph::from_documents(docs: &[DocMeta]) -> Self` — pure-data test constructor. Filters `DocMeta.related` by `RelationType` to construct typed edges. No `Store` dependency, easy fixtures.
- Edge accessors return filtered iterators by `EdgeKind`: `pub fn blocks_edges(&self) -> impl Iterator<Item=(NodeRef, NodeRef)>`, same for `implements_edges`. AC1 satisfied via filter, not via two parallel structures.
- Verify: `cargo build`.

### 3. Cycle check
- ACs: 2, 3.
- File: `src/engine/sequencing.rs`.
- `pub fn cycle_check(&self) -> Result<(), CycleError>`.
- Per Q-grilling decision "everything is a DAG": cycle check applies to ALL edges (both `blocks` AND `implements`); neither should cycle. Use `petgraph::algo::tarjan_scc` over the full graph, collect any SCC w/ size > 1 (or self-loop) as offending nodes.
- Verify: `cargo test sequencing::tests::cycle`.

### 4. Topo order
- ACs: 4.
- File: `src/engine/sequencing.rs`.
- `pub fn topo_order(&self) -> Result<Vec<NodeRef>, CycleError>`.
- Use `petgraph::algo::toposort` over the full graph (`blocks` + `implements` together; both contribute ordering). Map error to `CycleError`.
- Verify: `cargo test sequencing::tests::topo`.

### 5. Type-aware terminal helper (HARDCODE; Story 2 migrates to TypeDef config)
- ACs: 10, 11.
- File: `src/engine/sequencing.rs`.
- `pub fn is_terminal(doc: &DocMeta) -> bool`. Match on `doc.doc_type.as_str()` then `doc.status`.
  - `rfc` | `story`: `Complete | Superseded | Rejected`.
  - `iteration` | `audit`: `Complete`.
  - `adr` | `convention` | `dictum`: `Accepted | Superseded`.
  - default: false.
- Iteration 2 will reuse this; keep signature stable + free function (no `&self`).
- Source of truth: hardcoded match per RFC-041 in this iteration. STORY-124 (priority + TypeDef config) will migrate to `TypeDef::terminal_statuses` config field, replacing this hardcode w/ config lookup. Do NOT add config plumbing here.
- Verify: `cargo test sequencing::tests::terminal`.

### 6. Critical-path
- ACs: 9.
- File: `src/engine/sequencing.rs`.
- `pub fn critical_path(&self, scope: Scope, weights: &Weights) -> Vec<NodeRef>`.
- Algo: longest-path DP over topo order of `blocks`-filtered subgraph. Per node N, `best[N] = weight(N) + max(best[predecessor])` walking only `Blocks` edges. Trace back parent pointers. Default node weight 0 if missing from `Weights`.
- Scope filter: `All` = full graph; `Under(id)` / `After(id)` = subgraph reachable per RFC defs (sufficient: implement `All` fully, accept `Under`/`After` w/ TODO if blocks ordering only — confirm w/ minimum impl that satisfies AC9's "honours blocks ordering"). Subagent may stub `Under`/`After` to `All` if AC9 fixture uses `All`; tests only cover `All` here.
- Verify: `cargo test sequencing::tests::critical_path`.

### 7. Module registration
- ACs: ties everything together.
- File: `src/engine.rs`.
- Add `pub mod sequencing;` alongside existing `pub mod validation;` etc.
- Verify: `cargo build`.

### 8. Tests
- ACs: 1, 2, 3, 4, 9, 10, 11.
- File: `src/engine/sequencing.rs` bottom — `#[cfg(test)] mod tests`.
- Fixture helper: `fn doc(id: &str, ty: &str, status: Status, blocks: &[&str], implements: &[&str]) -> DocMeta`. Builds `DocMeta` w/ `id` + `related` populated.
- Tests per AC per `## Test Plan`. Arrange/act/assert layout. No shared mutable state.
- Verify: `cargo test sequencing`.

## Notes

### Decisions from grilling pass

- **Q1 — DRY w/ Store:** `Graph` reuses `Store`'s existing `forward_links_for` / `reverse_links_for` typed adjacency. Production ctor is `Graph::from_store(&Store)`. `Graph::from_documents(&[DocMeta])` retained as test ctor for unit fixtures. No duplicate adjacency representation.
- **Q2 — Single petgraph:** One `petgraph::Graph<NodeRef, EdgeKind>` w/ `EdgeKind = {Blocks, Implements}`. Filter by edge weight per kind-specific query. Cycle check applies to full graph (everything is a DAG; neither edge kind should cycle).
- **Q3 — context migration:** STORY-NEW (sibling under RFC-041) migrates `cli/context.rs::resolve_chain` onto the new `Graph`. NOT in this iteration. Keep STORY-120 pure-engine.
- **Q4 — terminal status:** Hardcode in iter 1 per RFC. STORY-124 (priority + TypeDef config) migrates to `TypeDef::terminal_statuses` once that touches config plumbing. Two-step migration; iter 1 stays small.

### Codebase anchors

- `Document` in code = `DocMeta` (`src/engine/document.rs`). Plan uses `DocMeta`.
- Doc-type config lives in `TypeDef` under `DocumentConfig` (`src/engine/config.rs`). No `terminal_statuses` field today. STORY-124 adds it.
- Relations on `DocMeta.related: Vec<Relation>` w/ `RelationType::{Implements, Blocks, Supersedes, RelatedTo}`. Iter ignores `Supersedes` + `RelatedTo`.
- Module declaration site is `src/engine.rs` (NOT `src/engine/mod.rs`).
- `Store::forward_links_for` / `reverse_links_for` / `children_of` / `parent_of` already exist — `Graph::from_store` borrows these.
- Audit type not in default `TypeDef` list. `is_terminal` matches by string so future audit types Just Work.
- petgraph not yet a dep — Task 1 adds it.
- Iter 2 consumes `is_terminal`, `cycle_check`, `topo_order`, `Graph` accessors. Keep all pub.
