---
title: Smart-traversal next-ready lease integration bottlenecks
type: iteration
status: complete
author: agent
date: 2026-04-30
tags: []
related:
- implements: STORY-120
---



## Summary

Iter 2/2 of STORY-120. Builds on Iter 1 graph primitives (`Graph`, `cycle_check`, `topo_order`, `critical_path`, `is_terminal`). Adds smart `next_ready` traversal w/ kind classification, lease filter+annotate, top-3 bottleneck diag. Pure, in-memory, no I/O. No CLI/TUI/skills.

## Acceptance Criteria covered

- AC5: leaf candidate, no `implements`-descendants -> `claimable`.
- AC6: leaf candidate, non-terminal self, all sibling chains exhausted -> `needs-children`.
- AC7: candidate w/ all `implements`-descendants terminal but self non-terminal -> `needs-status-update`.
- AC8: candidate w/ incomplete `implements`-descendants -> hide candidate, surface ready descendants regardless of doc type.
- AC12: default `next_ready` excludes leased docs.
- AC13: `include_leased` opt-in returns leased docs annotated w/ lessee.
- AC14: result has top-3 bottleneck list, ordered by # downstream candidates gated.
- AC15: <3 entries when fewer gating non-terminals exist; no padding.

## Test Plan

All tests live in `#[cfg(test)] mod tests` at bottom of `src/engine/sequencing.rs`. Pure in-memory `Document` fixtures. No tempfile/I-O. Reuse Iter 1 fixture builders if introduced.

- AC5 — arrange: 2 unblocked leaf docs, no `implements`-children, non-terminal status. Act: `next_ready(default opts)`. Assert: both returned, kind == `Claimable`.
- AC6 — arrange: leaf doc, non-terminal status, no `implements`-children, no other ready chain. Act: call. Assert: kind == `NeedsChildren` (signals "story exists w/ no iterations yet").
- AC7 — arrange: parent doc, non-terminal; 2 `implements`-children, both terminal status per type rules. Act: call. Assert: parent surfaces w/ kind == `NeedsStatusUpdate`; children NOT in result.
- AC8 — arrange: parent (e.g. story) w/ 1 terminal child + 1 non-terminal child. Act: call. Assert: parent absent; non-terminal child returned as `Claimable`. Cover doc-type variant: parent of type rfc w/ child of type story -> still descend.
- AC12 — arrange: ready candidate marked leased via injected lease set. Act: `next_ready` w/ default opts. Assert: candidate absent.
- AC13 — arrange: same fixture as AC12. Act: opts w/ `include_leased = true`. Assert: candidate present, `lessee == Some("agent-x")`.
- AC14 — arrange: graph w/ ≥4 non-terminal docs each gating distinct downstream counts (e.g. 5, 3, 2, 1). Act: call. Assert: bottleneck list len == 3, order by descending gate count, IDs match top 3.
- AC15-a — arrange: graph w/ 1 non-terminal gating doc. Act: call. Assert: bottleneck list len == 1.
- AC15-b — arrange: graph w/ 0 gating non-terminals. Act: call. Assert: bottleneck list empty.
- Mixed — arrange: graph combining lease + bottleneck + descent (parent w/ leased child, terminal sibling, ungated descendant). Act: call. Assert: result reflects all rules together (regression guard).

## Changes

1. Add types in `src/engine/sequencing.rs`:
   - `pub struct NextOpts { pub include_leased: bool, pub scope: Option<DocId> }` w/ `Default`.
   - `pub enum ReadyKind { Claimable, NeedsChildren, NeedsStatusUpdate }`.
   - `pub struct ReadyCandidate { id, kind, lessee: Option<String> }`.
   - `pub struct Bottleneck { id, gates: usize }`.
   - `pub struct NextResult { ready: Vec<ReadyCandidate>, bottlenecks: Vec<Bottleneck>, warnings: Vec<GraphWarning> }`.
   - `pub enum GraphWarning { ... }` (extend Iter 1's if exists; else new).
   - Note: Iter 1 ships a single `petgraph::Graph<NodeRef, EdgeKind>` w/ `EdgeKind = {Blocks, Implements}`. Edge-kind filter accessors expose `blocks_*` / `implements_*` views. Use those — do NOT reach into petgraph internals.

2. Impl smart-traversal `pub fn next_ready(graph: &Graph, opts: &NextOpts, leases: &LeaseView) -> NextResult` in `src/engine/sequencing.rs`:
   - Compute candidates: nodes w/ all `blocks`-upstreams terminal (via Iter 1 `is_terminal` + edge-kind filter on `Blocks`) AND not themselves terminal.
   - For each candidate: enumerate `implements`-descendants via edge-kind filter on `Implements`.
     - Any non-terminal descendant: hide self, recursively surface ready descendants regardless of doc type (AC8).
     - All descendants terminal + self non-terminal: `NeedsStatusUpdate` (AC7).
     - No descendants + self non-terminal: classify via leaf-kind heuristic (see Notes) — `Claimable` (AC5) for terminal-work types (iteration, audit), `NeedsChildren` (AC6) for decomposing types (rfc, story, etc.). Hardcode RFC-041 type table in this iter; ADR follow-up to migrate onto `TypeDef` config (mirrors Q4 path for terminal_statuses).
   - Dedupe by id.

3. Lease integration. Define `pub struct LeaseView { held: HashMap<DocId, String /*agent*/> }` in `src/engine/sequencing.rs` to keep `next_ready` pure. Caller (later story) builds this from `LeaseEngine::query`. Inside `next_ready`:
   - Default: skip candidate if `leases.held.contains_key(&id)` (AC12).
   - `include_leased`: keep candidate, populate `lessee` from `held` map (AC13).

4. Bottleneck diag: walk all non-terminal nodes; for each, count downstream candidates whose `blocks`-upstream chain transits this node. Sort by count desc, take top 3 (AC14). If fewer than 3 gating non-terminals, return only those (AC15). Tie-break by document id ascending for determinism.

5. Unit tests in `#[cfg(test)] mod tests` at bottom of `src/engine/sequencing.rs` per Test Plan above. DICTUM-004: isolated, behavioral, structure-insensitive, deterministic, specific, readable. Fixed `Document` fixtures; no I/O; reuse Iter 1 fixture helpers.

## Notes

### Lease module API surface (`src/engine/lease.rs`)

- `pub struct Lease { pub agent: String, pub acquired: DateTime<Utc>, pub expires: DateTime<Utc> }`.
- `pub struct LeaseEngine<R: GitRefOps> { pub git: R, pub config: CoordinationConfig }`.
- Methods: `acquire`, `release`, `admin_release`, `heartbeat`, `force_acquire`, `query(root) -> Result<Vec<(String /*refname*/, Lease)>>`.
- Refname format: `refs/lazyspec/leases/{type}/{id}` — extract `(type, id)` to map back to `DocId`.
- All methods take `&Path` + do git I/O. NOT pure.

### Implication for sequencing purity

`next_ready` MUST stay pure (DICTUM-004 + Story scope: no I/O). Inject lease state via `LeaseView { held: HashMap<DocId, String> }` constructed by caller. Caller calls `LeaseEngine::query`, parses refnames, builds `LeaseView`, passes in. Keeps `sequencing.rs` testable w/ fixed in-memory fixtures.

### Document neighbour exposure (`src/engine/document.rs`)

- `DocMeta.related: Vec<Relation>` flat list.
- `Relation { rel_type: RelationType, target: String }`.
- `RelationType::{Implements, Supersedes, Blocks, RelatedTo}`.
- No prebuilt neighbour helpers — Iter 1's `Graph` builder filters `related` by `rel_type` to construct typed petgraph edges.

### Leaf-claimable heuristic (AC5 vs AC6) — RESOLVED via grilling

Distinguishing `Claimable` from `NeedsChildren` for childless leaf depends on doc type. RFC-041 type table:
- `iteration`, `audit` -> `Claimable` (terminal-work types; childless leaf IS the unit of execution).
- `rfc`, `story` -> `NeedsChildren` (decomposing types; childless leaf needs `/create-story` or `/plan-work`).
- `adr`, `convention`, `dictum` -> not work-bearing; treat as `Claimable` if reached (defensive; RFC scope says these don't appear as ready candidates in practice).

Implementation: hardcode this table in iter 2 (mirrors Q4 path for terminal_statuses). ADR follow-up captures intent. Migrate onto `TypeDef::decomposes_into` (or similar) when STORY-124 or a successor proves the second use.

### Determinism

All sort/iteration order over `HashMap` must be replaced w/ `BTreeMap` or post-sort by id to satisfy DICTUM-004 deterministic tests.
