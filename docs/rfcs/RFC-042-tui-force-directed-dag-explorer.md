---
title: "TUI force-directed DAG explorer"
type: rfc
status: draft
author: "jkaloger"
date: 2026-05-01
tags: [tui, dag, sequencing, visualization]
---

## Intent

RFC-041 ships layered (Sugiyama) DAG render for sequencing edits. Layered reads topo order well, weak at cluster discovery. Add second render mode: force-directed, terminal-native, optimized for *exploring* the graph (cluster discovery via tag affinity, aesthetic overview, "what hangs together"). Editing stays in layered mode; force mode is read-mostly with optional drag-pin.

Primary jobs:

1. See tag-cohorts as visual clusters without manual filtering.
2. Spot orphans, bridges, dense subgraphs at a glance.
3. Aesthetic dashboard / status-of-project view; pleasant enough to live on a side monitor.

Non-jobs (deferred to RFC-041 layered or future `serve` RFC):

- Inline DAG editing (add/remove blocks edges). Layered owns this.
- Multi-user / web hosting. Possibly own RFC if demand emerges.
- Animation of historical evolution (replay `next_ready` over time). Future.

## Non-goals

- Sub-cell pixel graphics over kitty/sixel/iterm2 protocols. Terminal-native = cell-grid only. ssh-safe, Terminal.app-safe, no protocol gating.
- Replacing layered. Layered remains default.
- Custom force-sim implementation. Use ecosystem crate.
- Mouse-first interaction. Keyboard primary, mouse opt-in via crossterm capture.

## Constraints

- Engine layer stays I/O-free. Layout = pure function `&[Document] -> Vec<NodePosition>`.
- TUI layer owns rasterisation. CLI gets nothing new (force layout is interactive-only; CLI graph export stays d2/dot via RFC-041).
- Principle 5: prefer ecosystem crate for force-sim. Principle 6: no trait abstraction over render markers; one impl ships, swap if second emerges.

## Design

Three concerns, evaluated independently: layout algorithm, render primitive, edge routing.

### Layout algorithm

Candidates (Rust):

| Crate | Algo | Status | Notes |
|-------|------|--------|-------|
| `fdg` | Fruchterman-Reingold + variants | maintained, ~v0.5 | headless, returns coords; integrates with `petgraph` types |
| `forceatlas2` | ForceAtlas2 (Gephi) | thin port | dense-graph oriented, overkill for ~50-node DAGs |
| custom Verlet | spring + repulsion | n/a | ~150 LOC, full control over constraints |
| `petgraph-evcxr` etc | n/a | n/a | display helpers, not layout |

Decision: `fdg` (Principle 5: ecosystem norm, integrates with `petgraph` already in RFC-041). Wrap as `engine::layout::force::ForceLayout` returning `Vec<(NodeId, f32, f32)>`. Sim runs to convergence (KE threshold) on view enter, freezes. Re-anneal on graph mutation or explicit `r` keypress.

Optional later: tag-affinity force (extra spring between same-tag pairs) as a one-line addition once base lands.

### Render primitive

ratatui supports four `Marker` modes on `Canvas`: `Block`, `Dot`, `HalfBlock`, `Braille`. Plus non-canvas: direct `Buffer` writes with box-drawing chars.

| Marker | Resolution per cell | Color per pixel | Best for |
|--------|---------------------|-----------------|----------|
| `Block` (█) | 1×1 | yes | chunky fills |
| `HalfBlock` (▀▄) | 1×2 | yes (top + bottom independently) | smooth horizontal-ish curves, good color |
| `Dot` (•) | 1×1 sparse | yes | scatter, no continuous lines |
| `Braille` (⠁⠂⠄⡀⢀⠐⠈⠠ …) | 2×4 | one fg per cell (8 dots share color) | smoothest geometry, but cell-uniform color |
| Box-drawing direct | n/a | per-char | rectilinear / hierarchical, not curves |

Color limitation matters for cluster discovery: braille's 8 dots share one fg color per cell, so an edge crossing a cluster hull glitches color. HalfBlock allows top/bottom independently. Tradeoff: HalfBlock has 1×2 resolution vs braille's 2×4 — coarser geometry, finer color.

Decision: ship `HalfBlock` as default for the explorer. Color per pixel matters more than geometric smoothness for cluster discovery (the primary job). Braille remains available as a `--marker braille` toggle for users on monochrome terminals or who prefer geometric fidelity. No trait — `enum Marker` switch in the renderer (Principle 6: two concrete uses qualifies for indirection).

Rejected: kitty/sixel/iterm2 image-blit (out of scope per non-goals; ssh-fragile, protocol-gated). Direct box-drawing: too rectilinear for force-directed; defeats the aesthetic.

### Edge routing

Naive: straight-line per edge, rasterise via Bresenham over `HalfBlock` grid. Works for ~30 edges; degrades on dense crossings.

Better: route around node bboxes (force sim places nodes; edges shouldn't cross labels). Two passes:

1. Plot node bboxes into an obstacle map.
2. For each edge, if straight line clips a non-endpoint bbox, run A* on the marker grid with bbox cells as obstacles. Otherwise straight.

A* on a 200×60 grid = trivial. Skip on first iteration; add when overlap becomes visible.

Crossings: when two edges share a marker cell, lighten the lower-priority edge (dim color) so the user can trace the dominant one. Edge priority = critical-path > selected-ego > rest.

### Cluster hulls

Per-tag convex hull over member node positions, padded ~2 cells, traced with `╭─╮ ╎ ╰╌╯` rounded box-drawing chars (cell-grid, not marker). Hull line uses tag color. Multiple-tag membership: node belongs to its primary tag (first in frontmatter); secondary tags surface in detail pane.

Hard groups (RFC-041 future, `group:` frontmatter or RFC-implements-RFC parent) render as nested constrained sub-region. Out of scope for v1; hook the layout function so a follow-up RFC adds nesting without rewriting.

### Tag colors

Stable hash(tag-string) → 16-color ANSI cycle, skipping reds (red reserved for cycle-violation / blocked status). User override in `lazyspec.toml`:

```toml
[tag_colors]
sequencing = "cyan"
agents     = "magenta"
tui        = "yellow"
storage    = "green"
```

Legend in status bar; full mapping via `?` overlay.

### Interaction

Read-mostly. Keys:

- `f` / `l` toggle force / layered (shared between modes).
- arrows pan, `+` / `-` zoom (sim coords scale, marker re-rasters).
- `tab` cycle nodes; `enter` focus selected (highlight ego graph, dim rest).
- `r` re-anneal sim from current positions.
- `g` cycle group view: none → tag-hull → hard-group (future).
- `c` toggle critical-path overlay (RFC-041 spec).
- `m` cycle marker: halfblock → braille → block.
- mouse drag (when capture on): pin node, sim relaxes around it. Click background = unpin all.

No add/remove edge keys in this mode; switch to layered for editing. Status bar reminds.

### Layering

```
engine::layout::force::ForceLayout       // pure, fdg wrapper
engine::layout::hull::TagHulls           // pure, convex hull per tag
tui::views::dag::explorer                // new view
tui::views::dag::raster::{halfblock, braille, block}  // marker impls
tui::views::dag::route                   // straight + A* fallback
```

Engine returns positions only. TUI owns all rendering. CLI untouched.

## Interfaces

```rust
// @draft engine/layout/force.rs
pub struct ForceLayout {
    pub positions: Vec<(DocumentId, f32, f32)>,
    pub bounds: (f32, f32, f32, f32), // (minx, miny, maxx, maxy)
    pub iterations: u32,
    pub converged: bool,
}

pub struct ForceOpts {
    pub max_iters: u32,        // default 500
    pub ke_threshold: f32,     // default 0.01
    pub tag_affinity: bool,    // default false in v1
    pub seed: Option<u64>,     // deterministic for tests
}

pub fn force_layout(graph: &Graph, opts: ForceOpts) -> ForceLayout;

// @draft engine/layout/hull.rs
pub struct TagHull {
    pub tag: String,
    pub points: Vec<(f32, f32)>, // CCW convex hull, padded
}

pub fn tag_hulls(positions: &ForceLayout, docs: &[Document]) -> Vec<TagHull>;
```

```rust
// @draft tui/views/dag/explorer.rs
pub enum Marker { HalfBlock, Braille, Block }

pub struct ExplorerState {
    pub layout: ForceLayout,
    pub hulls: Vec<TagHull>,
    pub marker: Marker,
    pub selected: Option<DocumentId>,
    pub pinned: HashSet<DocumentId>,
    pub crit_path: bool,
    pub group_mode: GroupMode,
}
```

Reuses `@ref engine::sequencing::Graph` (RFC-041). No new engine concepts beyond layout.

## Stories

1. **Engine: force layout via fdg.** Wrap `fdg` returning `ForceLayout`. Deterministic seed for tests. Pure, no I/O. Convergence detection. Unit tested with golden-position fixtures.

2. **Engine: tag hulls.** Convex hull per primary-tag cohort. Padding param. Pure.

3. **TUI: HalfBlock raster + straight-line edges.** New `dag::explorer` view. Renders nodes as labeled cell-rects, edges as HalfBlock lines via Bresenham, hulls as rounded box-drawing. No A* yet. Integrate `f`/`l` toggle with layered (RFC-041 Story 4).

4. **TUI: marker switch.** `m` key cycles HalfBlock / Braille / Block. Each marker re-rasters from same `ForceLayout`.

5. **TUI: interaction.** Pan, zoom, tab-cycle, focus/ego highlight, `r` re-anneal, `c` critical-path overlay (consumes RFC-041 critical-path output).

6. **TUI: edge routing fallback.** A* around node bboxes when straight line clips. Crossing dim heuristic.

7. **TUI: tag colors + legend.** Hash-based default palette; `[tag_colors]` TOML override; `?` overlay shows full mapping.

8. **Optional: tag-affinity force.** Extra spring per same-tag pair; toggle in status bar; default off until proven useful.

Stories 1, 2, 3 unblock the rest. 4–7 parallel. 8 deferred until 3–7 ship and dogfood reveals whether default sim already clusters enough.

## ADR candidates

- **`fdg` for force layout** (Principle 5: ecosystem norm; alternative custom-Verlet rejected per Principle 6 until two uses).
- **HalfBlock as default marker, Braille opt-in** (color-per-pixel beats geometric resolution for cluster-discovery job; both ship).
- **Force mode is read-mostly; editing stays in layered** (separation of jobs: explore vs edit; one TUI surface per job).
- **No graphics protocols (sixel/kitty/iterm2)** (terminal-native, ssh-safe, no protocol gating; revisit if `serve` RFC lands).

## Open questions / deferred

- **Hard-group rendering** (compound nodes for RFC-implements-RFC nesting). Hook in layout API; visual story deferred.
- **Edge bundling** for dense crossings. Worth evaluating once project DAG exceeds ~80 edges.
- **Animation of state evolution** (replay sequence of `next_ready` over commit history). Demo-grade feature; separate RFC if requested.
- **`lazyspec serve` web target.** Different RFC. Force-directed in browser via cytoscape/d3 covers the "best aesthetics" niche. This RFC scoped strictly to terminal-native.
- **Mouse-drag pin UX.** Crossterm mouse capture optional; default keyboard-only. Decide after dogfooding.
