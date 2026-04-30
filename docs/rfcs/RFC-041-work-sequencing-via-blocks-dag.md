---
title: "Work sequencing via blocks DAG"
type: rfc
status: draft
author: "jkaloger"
date: 2026-04-30
tags: [sequencing, dag, planning, tui, agents]
---

## Summary

Treat the typed `blocks` relationships across documents as a directed acyclic graph (DAG) and expose it as the primary work-sequencing model. The DAG drives three consumers: humans picking work, agents/swarms claiming iterations autonomously, and a TUI sequencing screen that replaces the existing graph view. No new relationship types. A typed `priority` frontmatter field replaces ad-hoc tier tagging, with the priority vocabulary configurable in `lazyspec.toml` (default: MoSCoW).

## Problem

Today, lazyspec encodes dependencies via the `blocks` relationship (ADR-003) but treats it as flat metadata. There is no first-class concept of:

1. **What is ready to work on now.** A human or agent must read frontmatter, walk relationships manually, and infer ordering.
2. **What blocks delivery.** No critical-path query. Foundational work (mocks, stubs, atomic interfaces) is not distinguished from feature slices.
3. **What is unlocked by completing a feature.** No forward-traversal query: "if I finish STORY-X, what becomes deliverable next?"
4. **A planning artifact.** Teams that plan a project horizontally (brainstorm RFCs and stories together, then sequence for delivery) have no surface for the sequencing pass. Sequencing is implicit in commit order.
5. **Agent orchestration substrate.** RFC-036 envisions claim-based multi-agent execution, but agents have no canonical "what is next" query.

The existing graph view (STORY-015) renders an `implements` tree with `blocks` as inline annotations. It is read-only and tree-shaped, which obscures the DAG structure that matters for sequencing.

## Design Intent

`blocks` already forms the DAG. This RFC adds a graph layer in `engine`, three CLI surfaces over it, an interactive TUI sequencing screen that replaces the current graph view, and two skills that consume the CLI. A new `priority` frontmatter field makes critical-path weighting explicit and validated.

### Graph model in engine

The graph is derived from documents on each query. Nodes are all documents; edges are `blocks` and `implements` typed separately. Both surfaces (this RFC and `lazyspec context`) share the same `engine::Graph` substrate; `context` walks `implements` only, sequencing walks both.

```rust
// @draft engine/sequencing.rs
pub struct Graph {
    nodes: Vec<NodeRef>,
    blocks_edges: Vec<(NodeRef, NodeRef)>,
    implements_edges: Vec<(NodeRef, NodeRef)>,
}

pub enum Scope {
    All,
    Under(DocumentId),  // implements descendants + blocks ancestors (transitive)
    After(DocumentId),  // blocks descendants (transitive)
}

impl Graph {
    pub fn from_documents(docs: &[Document]) -> Self;
    pub fn cycle_check(&self) -> Result<(), Vec<NodeRef>>;
    pub fn topo_order(&self) -> Result<Vec<NodeRef>, CycleError>;
    pub fn next_ready(&self, scope: Scope, opts: NextOpts) -> NextResult;
    pub fn critical_path(&self, scope: Scope, weights: &Weights) -> Vec<NodeRef>;
}
```

#### Terminal status (when is an upstream `blocks` cleared?)

An upstream is "done" iff its status is in its type's terminal set. Type-aware (engine reads from doc-type config):

- RFC, Story: `{complete, superseded, rejected}`
- Iteration, Audit: `{complete}`
- ADR, Convention, Dictum: `{accepted, superseded}`

`accepted` is terminal only for decision artifacts. For work-item types it means "design ready to start," not "done." Validation surfaces the common drift: RFCs marked `accepted` whose implementing stories are all `complete` get a warning suggesting promotion to `complete`. This drives lifecycle cleanup organically without a project-wide audit.

A canonical lifecycle ADR is still desired but does not block this RFC.

#### Smart traversal (no work-item type filter)

`next_ready` does not filter by document type. Instead, for each ready candidate it descends `implements`-children:

- If children exist, recurse into them and return their ready descendants.
- If no children, return self.

Each result carries a `kind`:

- `claimable` — terminal-eligible blockers cleared, leaf in implements tree. Hand off to `/build`.
- `needs-children` — ready, no implements-descendants. Hand off to `/create-story` or `/plan-work`.
- `needs-status-update` — all implements-descendants are terminal but self is not. Hand off to a human or auto-bump.

This means RFCs without stories surface as work-to-plan; RFCs with stories transparently delegate to their ready stories; iterations surface as claimable. No `work_item: bool` configuration needed; doc types remain opaque.

#### Lease interaction

By default, `next_ready` filters out documents with active leases (RFC-035). `--include-leased` flag returns them with a `leased_by` field. TUI sequencing screen overview shows leased docs decorated with the lessee's identity.

#### Bottleneck diagnostic

Every `next` payload includes a `bottlenecks` field listing the top three non-terminal documents that gate the largest number of downstream candidates. Free to compute (graph already built); guides where to push when the ready set is small or empty.

#### Cycle handling

Cycles are validation errors but not catastrophic. `validate` flags them. `next` / `graph` / `critical-path` skip cycle-affected nodes and emit a `warnings` field with cycle membership. TUI inline edits prevent new cycles by rejecting any `blocks` edge that would induce one.

#### Critical path weights

`critical_path` weights nodes by `priority`. Weights are read from `lazyspec.toml`:

```toml
[priorities.must]    weight = 4
[priorities.should]  weight = 3
[priorities.could]   weight = 2
[priorities.wont]    weight = 1
```

Default config is MoSCoW above. Users may redefine the priority set entirely (different keys, different counts, different weights). Engine validates the `priority` frontmatter field against configured keys.

Engine uses `petgraph` for the underlying graph (Rust ecosystem norm, Principle 5).

### CLI

Three new commands, all `--json`-capable:

```text
lazyspec next           [--scope <id>] [--after <id>] [--type <doc-type>] [--include-leased] [--json]
lazyspec graph          [--scope <id>] [--after <id>] [--format d2|json|dot]
lazyspec critical-path  [--scope <id>] [--after <id>] [--json]
```

`--scope` and `--after` are mutually exclusive. `--scope` only accepts documents that have implements-descendants (RFC, Story); iteration as scope is rejected with a hint.

`validate` gains:

- Cycle check (error).
- "RFC accepted but all implementing stories complete" (warning) — drives lifecycle cleanup.
- Rejected upstream blocker (warning) — surfaces stale planning.

The sequencing TUI prevents creating cycle-inducing edges inline.

### TUI: interactive sequencing screen

Replaces STORY-015 graph mode. Single graph screen, no implements-tree mode toggle (the `context` command and existing relations panel cover the tree-shape question).

Capabilities:

- Render the full DAG using existing layered layout primitives. Nodes coloured by `priority` and decorated with status.
- Scope filter: whole / under <id> / after <id>. Non-scope nodes rendered dimmed for whole-project orientation; in-scope highlighted.
- Add a `blocks` edge by selecting source then target. Engine rejects cycle-inducing edges with a status-bar error.
- Remove a `blocks` edge by selecting it and pressing delete.
- Set `priority` on the selected node via numeric keys (`1`–`9` map to priority slots in `lazyspec.toml` definition order). Falls back gracefully when fewer than 9 priorities are defined; collisions with existing keybinds avoided (`p` is reserved for provenance editor).
- Edits are atomic: each add/remove/priority change writes via the engine link/unlink/update path. Undo unwinds the last N session ops.
- Budget panel shows cumulative priority counts: how many `must` / `should` / `could` / `wont` are in scope, what is `done` vs `remaining`.
- Critical path overlay toggle highlights the longest weighted path.

Read-only `lazyspec graph --format d2` output remains useful outside the TUI (CI artifacts, markdown embeds via RFC-033 pipeline).

### Skills

`/sequence`: opens the TUI sequencing screen on the chosen scope. Used during the sequencing pass after horizontal planning.

`/next-work`: queries `lazyspec next --json`, presents the candidate set to the human, asks which to claim, then claims via `lazyspec claim` and hands off to `/build` (or `/create-story` if the chosen candidate has kind `needs-children`). Human-in-the-loop by design; autonomous selection deferred to RFC-036 daemon flow.

Horizontal planning (`/plan-project`) is out of scope here; tracked as a separate RFC.

### Priority encoding

Replaces the prior "tier" sketch. A typed frontmatter field with a configurable vocabulary:

```yaml
priority: must
```

Required on documents whose type appears in `[doc_types.<type>]` with `requires_priority = true` (defaults: story, iteration). Optional elsewhere; falls back to the lowest-weight priority for critical-path computation when missing.

`lazyspec.toml`:

```toml
[priorities.must]    weight = 4
[priorities.should]  weight = 3
[priorities.could]   weight = 2
[priorities.wont]    weight = 1
```

Validation rejects unknown priority values. Tags remain free-form for semantic classification (security, ux, refactor) and are not conflated with priority.

## Interfaces

```rust
// @draft engine/sequencing.rs
pub enum Scope {
    All,
    Under(DocumentId),
    After(DocumentId),
}

pub struct Weights(HashMap<String, u32>);

pub struct NextOpts {
    pub include_leased: bool,
    pub type_filter: Option<Vec<String>>,
}

pub struct NextResult {
    pub ready: Vec<ReadyCandidate>,
    pub bottlenecks: Vec<Bottleneck>,
    pub warnings: Vec<GraphWarning>,
}

pub struct ReadyCandidate {
    pub id: DocumentId,
    pub kind: ReadyKind, // Claimable | NeedsChildren | NeedsStatusUpdate
    pub leased_by: Option<String>,
}

pub fn next_ready(docs: &[Document], scope: Scope, opts: NextOpts) -> NextResult;
pub fn critical_path(docs: &[Document], scope: Scope, weights: &Weights) -> Vec<&Document>;
pub fn graph_d2(docs: &[Document], scope: Scope) -> String;
pub fn cycle_check(docs: &[Document]) -> Result<(), CycleError>;
```

Reuses `@ref engine::store::Document`, `@ref engine::relationships::Relationship` for inputs. Doc-type config (terminal-status set, `requires_priority`) read from `@ref engine::config::DocTypeConfig`.

## Stories

1. **Engine: blocks DAG primitives.** Build `Graph` over documents using `petgraph`. Implement `cycle_check`, `topo_order`, smart-traversal `next_ready` with kinds, `critical_path`. Type-aware terminal status. Lease integration. Pure, no I/O. Unit tested.

2. **Engine + config: priority field and TOML config.** Add `priority` frontmatter field. Parse `[priorities.*]` from `lazyspec.toml` with MoSCoW defaults. Validate frontmatter values. Add `requires_priority` to doc-type config.

3. **CLI: `next`, `graph`, `critical-path` commands.** Wire engine primitives to subcommands with `--json` (including bottlenecks and warnings). Add `--scope` and `--after` flags. Add cycle check, accepted-but-children-done warning, and rejected-upstream warning to `validate`. Update help and README.

4. **TUI: interactive sequencing screen.** New screen replacing STORY-015 graph mode. Renders full DAG dimmed-by-scope, supports add/remove `blocks` edges with cycle prevention, scope filter (all / under / after), priority editing via numeric keys, status colouring, budget panel, critical-path overlay. Writes via engine.

5. **Skill: `/sequence`.** Opens TUI sequencing screen on chosen scope.

6. **Skill: `/next-work`.** Queries `lazyspec next --json`, presents candidates to human for selection, claims, hands to `/build` or `/create-story` based on kind.

7. **Retire STORY-015 graph mode.** Remove old graph rendering once new screen lands. Update keybindings and help.

Stories 1, 2, 3 unblock everything else. Story 4 depends on 1, 2, 3 plus RFC-033 layout primitives. Stories 5 and 6 depend on 3 (CLI). Story 7 depends on 4 landing.

## ADRs

- ADR: Reuse `blocks` for DAG edges (no new relationship type). Two-uses rule (Principle 6) not met for a new type; existing semantics fit.
- ADR: Typed `priority` frontmatter field (replaces tier-via-tags sketch). Vocabulary configurable in TOML, MoSCoW default. Multiple consumers on day one (critical-path weight, TUI colouring, budget panel, validate); typed encoding prevents the silent-typo footgun for downstream lazyspec consumers.
- ADR: Type-aware terminal status set. Rationale: `accepted` is terminal for decision artifacts but not for work items, where it means design-ready-to-start. Avoids forcing a project-wide lifecycle audit before sequencing can ship.
- ADR: `petgraph` for graph operations. Rust ecosystem norm (Principle 5).

## Open Questions / Deferred

- **Canonical lifecycle ADR.** Type-aware terminal sets handle v1 cleanly; a unified status-state-machine ADR is still wanted, tracked separately.
- **Horizontal planning skill (`/plan-project`).** Bulk RFC + Story drafting from a discovery session. Separate RFC. The DAG sequencing in this RFC is the consumer, not the producer.
- **Soft vs hard blockers.** Some teams want "soft after" (preference, not hard dep). Deferred. If it becomes load-bearing, introduce a second relationship type then, not before.
- **Custom priority TUI colours.** Defaults wired in code. Per-priority colour configuration in `lazyspec.toml` deferred until a project asks.
- **Daemon-driven autonomous next-work selection (RFC-036).** Skill currently asks human to pick. Daemon flow can call engine directly without the human-pick step.
