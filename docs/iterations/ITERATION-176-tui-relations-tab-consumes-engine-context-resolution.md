---
title: TUI relations tab consumes engine context resolution
type: iteration
status: draft
author: agent
date: 2026-06-18
tags:
- tui
- relations
- context
related:
- implements: STORY-124
---

## Goal

Make the TUI relations tab build its lineage, forward, and related sections from the
engine's `resolve_chain` (ITERATION-174) on the selected document, removing the tab's own
`implements` chain walk. The tab then shows all parents of a multi-parent document and a
related set consistent with the CLI `context` command. Delivers STORY-124.

Depends on ITERATION-174 (engine `src/engine/context.rs` with `resolve_chain`,
`ResolvedContext`). Do not start until that module exists.

## Changes

1. **Rebuild `relation_items` from the engine resolution.**
   - `relation_items` (`src/tui/state/app.rs:829`) builds an ordered `Vec<PathBuf>` of the
     relation targets: an upward `implements` chain (single-parent via `find_map` over
     `forward_links`, lines 832-859), reverse-`implements` children (861-868), and
     `related-to` forward+reverse (870-884). Replace the body with a call to
     `crate::engine::context::resolve_chain(&self.store, <doc id>, <depth>)` and flatten
     its result into the `Vec<PathBuf>` the tab already consumes.
   - Flatten order must be stable because `selected_relation`, `relation_count`,
     `move_relation_up/down`, and `navigate_to_relation` index into this vector. Define the
     order as: chain `nodes` (engine topological order, excluding the target itself),
     then `forward`, then `related`. Exclude the target document.
   - This is where the multi-parent fix lands: the engine chain carries all parents, so a
     doc that `implements` two parents lists both, replacing the `find_map` single-parent
     walk.
   - Depth: use the same default depth the CLI uses for related (`1`) so behaviour matches
     the tab today; no depth UI (out of scope).

2. **Drive the renderer from the same resolution.**
   - `render_relationship_sections` (`src/tui/views/panels.rs:975`) has its own duplicate
     `implements` chain walk (~lines 1000-1015) building `chain_paths`. Remove it and
     source the chain/forward/related sections from the same `resolve_chain` result (or
     from the rebuilt `relation_items`), so the rendered sections and the navigable item
     list are guaranteed identical. Preserve the existing section layout, headings, and
     the "No relations." empty state (`panels.rs:992`).
   - AC: empty state unchanged; single-parent + direct related-to presentation unchanged.

3. **Verify navigation stays consistent.**
   - Confirm `navigate_to_relation` (`src/tui/state/app.rs:909`) still resolves the
     selected index to the right document after the order change. No interface change to
     the navigation methods; they consume the rebuilt `relation_items`.

## Test Plan

- **`relation_items` unit tests, AC: tasks 1, 3.**
  Over an in-memory store: (a) multi-parent doc lists both parents in the chain section;
  (b) item order is exactly chain-then-forward-then-related with the target excluded;
  (c) the related set matches `resolve_chain(...).related` for the same doc. Isolated,
  deterministic (engine ordering is path-stable), specific (assert the exact path
  sequence so navigation indices are pinned).

- **Render parity test, AC: task 2.**
  For a fixture doc, assert the relations tab's rendered section contents (chain, forward,
  related membership) equal the `context <doc>` resolution. Behavioural; assert membership
  and section grouping, not byte-for-byte, so it tolerates unrelated styling.

- **Empty + single-parent regression, AC: task 2.**
  Existing relations-tab tests with no relations assert "No relations."; single-parent +
  direct related-to fixtures produce unchanged output. These guard backward compatibility.

- **Navigation test, AC: task 3.**
  With a multi-parent fixture, set `selected_relation` across the range and assert
  `navigate_to_relation` lands on the document at that flattened index.

## Notes

- After ITERATION-174 there are two consumers of the chain inside the relations tab:
  `relation_items` (navigation) and `render_relationship_sections` (display). Both must
  derive from one `resolve_chain` call so the list you navigate is the list you see.
  Today they are two separate walks that happen to agree; collapsing them to one source
  removes that hazard.
- The shorthand-resolution fix from ITERATION-174 also benefits the tab: a story's parent
  RFC will now appear in the chain section where the old `find_map` walk dropped it.
- Depth-N related-to in the TUI is deliberately out of scope; the tab uses depth 1. If a
  depth control is wanted later it is a separate story (the engine already supports it).
