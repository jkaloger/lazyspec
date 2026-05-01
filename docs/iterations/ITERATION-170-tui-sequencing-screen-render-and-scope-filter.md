---
title: TUI sequencing screen render and scope filter
type: iteration
status: complete
author: agent
date: 2026-05-01
tags: []
related:
- implements: STORY-121
---



## Goal

Add a new TUI sequencing screen that renders the full project DAG (`blocks` + `implements`) with edge-type distinction, status colouring, and a scope filter (`whole` / `under <id>` / `after <id>`). Read-only this slice. Old `ViewMode::Graph` rendering stays untouched until STORY-115.

## Test Plan

Tests target the public state API (key handling, scope mutators) and engine helpers. No mocks; use `Store` over `TempDir`. TUI render exercised via state shape + targeted unit checks on cell styling helpers.

- **AC1 (edge distinction):** engine unit test on layered-layout helper — given fixture with both `blocks` and `implements` edges, returned edge list tags each edge with its kind. Render-side unit on edge-glyph/colour helper asserts blocks vs implements produce distinct `Style`.
- **AC2 (status colour):** unit test on node-style helper covers each `Status` variant returning a distinct `Color`.
- **AC3 (`under <id>` highlight):** engine unit on `scope_membership` for `Scope::Under` — fixture with RFC -> Story -> Iteration plus a blocks-ancestor; result is `{descendants via implements} ∪ {transitive blocks-ancestors}`.
- **AC4 (`after <id>` highlight):** engine unit on `scope_membership` for `Scope::After` — fixture with chain A -> B -> C via blocks; given A, returns `{B, C}`.
- **AC5 (no scope = no dim):** TUI state test — with `scope = None`, every layout node carries `dim = false`.
- **AC6 (iteration rejected):** TUI state test — `set_scope(iteration_id)` returns an error variant, leaves prior scope intact, sets a status-bar message.
- **AC7 (read-only):** TUI state test — fire each editing key (`a` add edge, `d` delete edge, priority keys) on the new screen; assert no `Store` write and the doc on disk is unchanged across a temp-fs round-trip.

Tradeoff noted: AC1 render assertion checks the style, not pixel output — ratatui Buffer snapshots would be more behavioural but brittle against layout tweaks. Style-helper unit catches the regression that matters (kind -> visual mapping).

## Changes

1. **Engine: layered layout + scope membership.** File: `src/engine/sequencing.rs`. Add:
   - `pub fn layered_layout(&self) -> LayeredLayout` returning `{ layers: Vec<Vec<NodeRef>>, edges: Vec<(NodeRef, NodeRef, EdgeKind)> }`. Layer assignment via longest-path from sources over `blocks ∪ implements`. Existing `topo_order` is the basis. Skip cycle-affected nodes (consistent with RFC: cycles non-fatal, surfaced via warnings).
   - `pub fn scope_membership(&self, scope: &Scope) -> HashSet<NodeRef>`. `All` returns all nodes. `Under(id)` returns implements-descendants of `id` plus transitive blocks-ancestors of every member. `After(id)` returns transitive blocks-descendants of `id`.
   - `pub fn is_iteration(&self, id: &str) -> bool` helper used by TUI to reject scope.
   - Unit tests for layered layout (fixture covers both edge types, multiple layers, cycle skipping) and scope membership (Under, After).
   - **Verify:** `cargo test -p lazyspec --lib engine::sequencing` passes; new tests cover the ACs listed under AC1, AC3, AC4.

2. **TUI state: `Sequencing` view mode + scope state.** Files: `src/tui/state/app.rs`, `src/tui/state.rs`, new `src/tui/state/sequencing.rs`. Add:
   - `ViewMode::Sequencing` variant alongside existing `Graph` (do not remove `Graph`; STORY-115 retires it). Update `next()` cycle and `name()`.
   - `pub struct SequencingState { scope: Scope, layout: LayeredLayout, in_scope: HashSet<NodeRef>, scope_input: Option<String>, error: Option<String> }`. Held on `App` as `pub sequencing: SequencingState`.
   - Constructor `SequencingState::rebuild(store: &Store, scope: Scope)` that calls `Graph::from_documents` then `layered_layout` + `scope_membership`.
   - Mutators: `set_scope_under(&mut self, id, store)`, `set_scope_after(&mut self, id, store)`, `clear_scope(&mut self, store)`. Each rejects iteration ids using `Graph::is_iteration`, sets `error` and preserves prior scope on rejection.
   - **Verify:** unit tests in module bottom for AC5, AC6.

3. **TUI keys: sequencing screen handler.** File: `src/tui/views/keys.rs`. Add `handle_sequencing_key`:
   - `s` enters `under` scope-input mode; `f` enters `after`; `c` clears scope; `Esc` cancels input.
   - During scope-input: typing chars accumulates into `scope_input`; `Enter` calls `set_scope_*`; on rejection the error string surfaces in the status bar.
   - Edit gestures (`a`, `d`, numeric priority keys) are no-ops with an info message: "read-only screen". Wire into `keys.rs` dispatch alongside existing `handle_graph_key`.
   - **Verify:** unit test in `keys.rs` mod tests fires each editing key and asserts no `Store` mutation (AC7).

4. **TUI render: `draw_sequencing` panel.** File: `src/tui/views/panels.rs` + `src/tui/views/colors.rs`. Add:
   - `pub fn draw_sequencing(f, app, area)` rendering layered layout: each layer = horizontal row, nodes spaced evenly. Node label = `<id> <title>` styled by `status_color(status)`; dimmed (`Modifier::DIM` + `Color::DarkGray`) when `!in_scope`.
   - Edge rendering: `blocks` drawn as solid arrows (`──▶`) in one colour; `implements` as dashed (`╌╌▷`) in another. Distinct enough for AC1 — pick from existing palette in `colors.rs` (extend if needed).
   - Status-bar slot: scope summary (`scope: under STORY-121` / `none`) and any `error` from last action.
   - Wire `ViewMode::Sequencing => draw_sequencing(...)` in `src/tui/views.rs:202` dispatch.
   - **Verify:** unit on edge-style helper (AC1), unit on node-style helper across statuses (AC2).

5. **Status bar colour mapping.** File: `src/tui/views/status_bar.rs`. Add `ViewMode::Sequencing => Color::Magenta` (or pick an unused palette slot) so the mode indicator is distinct from `Graph`.
   - **Verify:** existing colour tests extended.

6. **Help text.** File: `src/tui/views/overlays.rs` (help screen). Add a section listing sequencing keys (`s`, `f`, `c`, navigation, read-only note).
   - **Verify:** manual via `cargo run`; help screen lists new keys.

## Notes

- Engine `Graph::critical_path` has a TODO at `src/engine/sequencing.rs:207` for scope filtering. Not needed this slice (no critical-path overlay). Leave the TODO; STORY-119 or critical-path slice handles it.
- Story refs "existing layered layout primitives" — none exist for graph layout. Building one inside engine (Task 1) since CLI `graph` command (later slice) will reuse it. Justifies the abstraction under principle 6 (two consumers).
- Old `ViewMode::Graph` and `draw_graph` (`panels.rs:1269`) untouched. STORY-115 retires.
- Iteration rejection (AC6): RFC says `--scope` rejects docs without implements-descendants, but the AC text specifically mentions iterations. Check `is_iteration` only for now; broader "no implements-descendants" rule belongs in CLI `--scope` slice.
- Edge glyphs: pure-ASCII fallback if terminal lacks unicode arrows — defer until a user reports it.

