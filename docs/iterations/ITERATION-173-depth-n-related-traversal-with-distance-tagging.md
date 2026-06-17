---
title: Depth-N related traversal with distance tagging
type: iteration
status: draft
author: agent
date: 2026-06-17
tags:
- cli
- context
- relationships
related:
- implements: STORY-122
- related-to: ITERATION-172
---

## Summary

Iteration B of STORY-122. Adds a `--depth N` flag (default 1) that bounds a BFS
over `related-to` links, so e.g. an ADR related to an RFC the chain relates to
surfaces at depth 2 (ADR-009). Tags every surfaced related (and forward)
document with `{relation, distance, via}` in `--json`.

Depends on iteration A (ITERATION-172): the multi-parent `nodes`/`target`
`ResolvedContext` shape and the JSON `forward`/`chain`/`target` keys must already
be in place. Build B only after A merges.

## Changes

### Task 1: `--depth` flag threaded to `resolve_chain`

ACs: "no flag → depth-1 behavior unchanged".

Files: `src/cli.rs` (`Context` variant, `src/cli.rs:187-195`), `src/main.rs`
(dispatch, `src/main.rs:259-266`), `src/cli/context.rs`.

- Add `#[arg(long, default_value_t = 1)] depth: usize` to the `Context` command
  variant.
- Thread `depth` through the dispatch (`src/main.rs:263,266`) into
  `run_json(&store, &id, depth)` and `run_human(&store, &id, depth)`, which pass
  it to `resolve_chain(&store, &id, depth)`.
- `resolve_chain` signature gains `depth: usize`. `depth == 1` must reproduce
  current related collection exactly (backward compatible default).
- Update README CLI reference for the new flag (per project instruction to keep
  the README in sync with the CLI interface).

Verification: `cargo build`; `lazyspec context <id>` (no flag) output identical
to pre-iteration; `--help` shows `--depth`.

### Task 2: BFS related traversal to N hops with distance/via

ACs: "depth 2 → ADR-via-RFC surfaces", "depth N → reachable within N hops appear,
deduped, none beyond N".

Files: `src/cli/context.rs` (related block, `src/cli/context.rs:62-94`).

- Replace the single-pass related collection with a BFS bounded by `depth`.
  Frontier 0 = all `nodes` (the chain/DAG). At each hop, follow
  `RelationType::RelatedTo` links (both `forward_links` and `reverse_links`) from
  the current frontier to undiscovered documents; assign each newly discovered
  doc `distance = hop` and `via = <path it was reached through>`. Stop after
  `depth` hops. Exclude documents already in `nodes`; dedup by path with a
  seen-set (first discovery wins, so `distance` is the shortest hop count).
- Change `related: Vec<&DocMeta>` to `related: Vec<RelatedRef<'a>>` where
  `pub struct RelatedRef<'a> { pub doc: &'a DocMeta, pub relation: RelationType,
  pub distance: usize, pub via: PathBuf }`. For `depth == 1` this yields the same
  document set as today, each tagged `distance = 1`.
- Apply the same `{relation, distance, via}` tagging to `forward` (distance 1,
  relation `Implements`) so forward and related are uniformly tagged; change
  `forward` to `Vec<RelatedRef<'a>>` or a shared tagged type.

Verification: tests below; `lazyspec context <id> --depth 2` surfaces the
two-hop doc; `--depth 1` set equals today's.

### Task 3: Tag related/forward in JSON and human output

ACs: "each surfaced related/forward doc carries relation, distance, via".

Files: `src/cli/context.rs` (`run_json`, `run_human`).

- `run_json`: each element of `related` and `forward` gets `relation`,
  `distance`, and `via` fields alongside the frontmatter object.
- `run_human`: in the `─── related ───` section, annotate entries reached beyond
  depth 1 with their distance (e.g. a trailing `(via SHORTHAND, d2)`); depth-1
  entries render as today to preserve the existing
  `related_records_in_human_output` expectation. Forward section unchanged
  visually.

Verification: `json_related_field_present` still passes; new tag tests below.

### Task 4: Update SPEC-012 for depth-N and tagging

Files: `docs/specs/SPEC-012-document-context-chain.md`.

- Extend "Related Records" to describe the `--depth N` BFS, shortest-distance
  dedup, and the `{relation, distance, via}` tags. Document the `depth == 1`
  default as the backward-compatible behavior.
- Update "JSON Output" to show the tagged related/forward entry shape.

Verification: `lazyspec validate --json` clean for SPEC-012.

## Test Plan

Integration tests through `resolve_chain` / `run_json` in
`tests/integration/cli_context_test.rs`, isolated and deterministic per the
testing dictum.

| AC | Test | Asserts |
|----|------|---------|
| Default depth unchanged | `context_depth_default_matches_today` | `resolve_chain(.., 1)` related set equals the pre-iteration depth-1 set for `setup_with_related`; each tagged `distance == 1` |
| Depth-2 surfaces two-hop | `context_depth_two_surfaces_adr_via_rfc` | Story relates-to RFC, ADR relates-to RFC; `resolve_chain(story, 2)` related contains the ADR with `distance == 2` and `via` = the RFC path |
| Depth bound | `context_depth_bounds_traversal` | A doc three hops out is absent at `depth 2`, present at `depth 3`; nothing beyond `depth` appears |
| Shortest distance on dedup | `context_related_shortest_distance` | A doc reachable at both 1 and 2 hops is recorded once with `distance == 1` |
| JSON tags present | `context_json_related_tagged` | Each `parsed["related"]` entry has `relation`, `distance`, `via`; same for `parsed["forward"]` |
| Forward tagged | `context_json_forward_tagged` | Forward implementors carry `relation == "implements"`, `distance == 1` |

Tradeoff: depth-bound and shortest-distance tests use small fixed fixtures with
distinct titles per hop so assertions read as arrange/act/assert without chasing
graph construction; the graph is built explicitly in each test, not shared.

## Notes

- Decision locked in ADR-009 (depth-bounded related traversal with distance
  tagging).
- Per-role depth policy (planning `--depth 2` vs build `--depth 1`) lives in the
  `resolve-context` skill, deferred to a follow-up, not this iteration.
- Fidelity tiering (summary vs full text by distance) is explicitly out of scope;
  this iteration only surfaces and tags, it does not trim content.
- `forward` and `related` share the `RelatedRef` tagged shape so JSON consumers
  parse one entry schema for both; `forward` is always `distance 1`,
  `relation = implements`.
