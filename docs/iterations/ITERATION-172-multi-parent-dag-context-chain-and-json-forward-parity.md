---
title: Multi-parent DAG context chain and JSON forward parity
type: iteration
status: accepted
author: agent
date: 2026-06-17
tags:
- cli
- context
- relationships
related:
- implements: STORY-122
---

## Summary

Iteration A of STORY-122. Replaces `lazyspec context`'s single-parent linear
chain walk with a multi-parent DAG resolved by BFS (ADR-008), and brings
`--json` to parity with human output by adding the dropped `forward` key.
Depth-N related traversal and `{relation,distance,via}` tagging (AC group 3) are
iteration B and out of scope here.

All edits are in `src/cli/context.rs` plus its tests and `SPEC-012`. The
`--depth` flag is NOT added in this iteration.

## Changes

### Task 1: Multi-parent BFS in `resolve_chain` + `ResolvedContext` shape change

ACs: "implements multiple parents → all ancestors appear", "diamond → shared
ancestor once", "cycle → terminates, each node once".

Files: `src/cli/context.rs`, `tests/integration/cli_context_test.rs`,
`tests/integration/cli_git_ref_context_test.rs`.

- Replace the `ResolvedContext` struct (`src/cli/context.rs:10-15`). Drop the
  linear `chain: Vec<&DocMeta>` and `target_index: usize`. Add:
  - `target: &'a DocMeta` — explicit reference to the requested document.
  - `nodes: Vec<ContextNode<'a>>` — every document reachable upward via
    `implements` (including the target), deduplicated by path, topologically
    ordered root-first so single-parent graphs serialize/render exactly as the
    old `chain` did.
  - a new `pub struct ContextNode<'a> { pub doc: &'a DocMeta, pub parents:
    Vec<PathBuf> }` where `parents` are the `implements` targets of `doc` that
    are themselves in `nodes` (the DAG edges).
  - keep `forward: Vec<&'a DocMeta>` and `related: Vec<&'a DocMeta>` unchanged.
- Rewrite the upward walk (`src/cli/context.rs:30-44`). Replace the
  `find_map`-over-first-`Implements` loop with BFS: seed a queue with the target,
  maintain a `HashSet<PathBuf>` seen-set, and for each dequeued doc follow ALL
  `RelationType::Implements` relations whose target resolves via `store.get`.
  Skip already-seen paths (this dedups diamonds AND guards cycles, fixing the
  latent infinite-loop in the current walk). Record each discovered parent path
  on the node's `parents`.
- Produce `nodes` in a deterministic topological order (root-first; ties broken
  by path for determinism per the testing dictum). For a single-parent graph
  this is the old `chain` order.
- The `forward` and `related` blocks (`src/cli/context.rs:48-94`) are unchanged
  in this iteration; `related` stays depth-1 (iteration B changes it).
- Migrate existing shape-coupled tests:
  - `cli_context_test.rs:26-30,39-41,125,158-160` — replace `resolved.chain`
    indexing and `resolved.target_index` with `resolved.nodes` (ordered docs)
    and `resolved.target`. `context_walks_full_chain` asserts the three docs in
    root-first order and `resolved.target.title == "Auth Sprint 1"`.
  - `cli_git_ref_context_test.rs:46-53,67-72,84-101` — same shape migration. The
    test at lines 84-101 documents the current single-parent-first behavior for
    a git-ref parent that is not loadable; under BFS the behavior for an
    unresolvable parent is unchanged (BFS still skips parents `store.get`
    cannot resolve), so update the assertions to the new shape but keep the
    "unresolvable parent is dropped" expectation, and remove the NOTE calling it
    a bug.
- New tests in `cli_context_test.rs` (see Test Plan): multi-parent fan-out,
  diamond dedup, cycle termination.

Verification: `cargo test --test integration cli_context cli_git_ref_context`
green; `cargo build` clean.

### Task 2: JSON output — `forward` key and DAG edge representation

ACs: "forward array present (populated and empty)", "JSON represents ancestor
set plus implements edges so the DAG is reconstructable".

Files: `src/cli/context.rs` (`run_json`, `src/cli/context.rs:104-118`),
`tests/integration/cli_context_test.rs`.

- Extend `run_json` to emit four top-level keys: `chain`, `forward`, `related`,
  `target`.
  - `chain`: array of node objects in `nodes` order. Each element is
    `doc_to_json_with_family(node.doc, store)` with an added
    `"implements_in_context": [<parent paths>]` field carrying that node's DAG
    edges, so a consumer reconstructs the graph without re-walking.
  - `forward`: array of `doc_to_json_with_family` for each `resolved.forward`
    doc. Empty array when there are no implementors (stable schema) — this is
    the parity fix; `forward` was previously dropped.
  - `related`: unchanged from today.
  - `target`: the target document's path string, replacing the old
    `target_index` so consumers can locate the requested doc in `chain`.
- No change to `doc_to_json` / `doc_to_json_with_family` signatures
  (`src/cli/json.rs:5,25`); the `implements_in_context` field is added by
  `run_json` after calling the helper.

Verification: new tests below green; `lazyspec context <id> --json | jq` shows
`forward` and `target`; existing `json_related_field_present`/`json_related_empty`
still pass.

### Task 3: Human render — tree for multi-parent, stack for single-parent

ACs: "single-parent renders as existing stack (backward compat)", "multi-parent
renders as indented tree, diamond drawn once, target carries marker".

Files: `src/cli/context.rs` (`run_human`, `mini_card`, `chain_connector`,
`src/cli/context.rs:120-254`).

- Detect the linear case: every node has at most one parent within `nodes`. When
  linear, render exactly the current vertical stack of mini-cards with `│`
  connectors and the `← you are here` marker on `resolved.target` (no behavior
  change; `you_are_here_marker` test must still pass).
- When some node has >1 parent (true DAG), render an indented tree rooted at the
  graph roots, descending along edges. Draw each node once (diamonds dedup by
  path); when a node is reachable by multiple paths, render it under its first
  encountered parent and elsewhere reference it by shorthand without redrawing
  the card. The target node still gets the `← you are here` marker.
- `forward` and `related` sections (`src/cli/context.rs:206-251`) render as
  today.

Verification: tests below green; manual `cargo run -- context STORY-122` (linear)
shows unchanged stack; a constructed multi-parent fixture shows the tree.

### Task 4: Update SPEC-012 to the new behavior

Files: `docs/specs/SPEC-012-document-context-chain.md`.

- Rewrite "Chain Resolution" to describe BFS over all `implements` relations,
  the dedup seen-set, cycle safety, and the topological `nodes` ordering;
  replace the `target_index` prose with `target`.
- Rewrite "JSON Output" to document `chain` (with `implements_in_context`
  edges), the new `forward` key, `related`, and `target`. Remove the sentence
  stating forward implementors are not surfaced in JSON.
- Update "Human Output" to describe the linear-stack vs indented-tree behavior
  and diamond dedup.
- Update `@ref` targets if any referenced symbol names changed
  (`ResolvedContext` fields).

Verification: `lazyspec validate --json` clean for SPEC-012; `@ref` directives
resolve.

## Test Plan

Tests are integration tests through the public `resolve_chain` / `run_json` /
`run_human` APIs in `tests/integration/cli_context_test.rs`, following the
existing fixture style (`TestFixture`, `write_doc`/`write_story`/etc.). All
fixtures are isolated and deterministic (fixed titles/dates, path-ordered
assertions) per the testing dictum.

| AC | Test | Asserts |
|----|------|---------|
| Multi-parent fan-out | `context_multi_parent_includes_all_ancestors` | A doc with two `implements` parents resolves `nodes` containing both parents and their ancestries; both lineages present |
| Diamond dedup | `context_diamond_ancestor_appears_once` | Two parents sharing a grandparent → grandparent appears exactly once in `nodes`; edges record both parent→grandparent links |
| Cycle termination | `context_cycle_terminates` | A implements B, B implements A → `resolve_chain` returns (no hang), each node once. Deterministic; no timeout needed since BFS seen-set bounds it |
| Single-parent backward compat (struct) | (migrated) `context_walks_full_chain`, `context_standalone_document` | `nodes` root-first equals old chain; `resolved.target` is the requested doc |
| Single-parent backward compat (render) | (existing) `context_human_output`, `you_are_here_marker`, `related_records_*` | Unchanged stack render, single marker on target, related section behavior intact |
| Forward in JSON populated | `context_json_forward_populated` | RFC with two implementing stories → `parsed["forward"]` has 2 entries with correct titles |
| Forward in JSON empty | `context_json_forward_empty` | Leaf iteration → `parsed["forward"]` is present and `[]` |
| DAG edges reconstructable | `context_json_edges_reconstructable` | Multi-parent fixture → each `chain` element carries `implements_in_context`; union of edges reconstructs the parent relation; `parsed["target"]` is the requested path |
| Multi-parent human tree | `context_human_tree_for_multi_parent` | Multi-parent fixture render contains all node titles, the target marker, and the shared ancestor title exactly once |

Tradeoff noted: the cycle test asserts termination + node-set, not output
ordering, because topological order is undefined for a cycle; asserting only the
deterministic facts (terminates, each node once) keeps the test predictive
without coupling to an arbitrary tie-break.

## Notes

- Decisions locked in ADR-008 (multi-parent DAG). ADR-009 (depth-N) is iteration B.
- The `--depth` CLI flag is NOT added here; `src/cli.rs` `Context` args are
  unchanged this iteration.
- TUI relations rendering (`src/tui/state/app.rs`, `src/tui/views/panels.rs`)
  reads `store.forward_links` directly, not `ResolvedContext`, so it is
  unaffected by the shape change and out of scope.
- Why `nodes` includes the target rather than a separate field: rendering and
  the "you are here" marker need the target positioned within the graph; a
  single ordered collection with an explicit `target` ref keeps both the linear
  and DAG cases uniform.
- Backward-compat hinge: topological root-first ordering makes single-parent
  `nodes` identical to the old `chain`, so only struct field names change for
  existing callers, not semantics.
