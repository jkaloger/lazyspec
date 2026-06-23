---
title: TUI honours config statuses; edges optional
type: iteration
status: complete
author: agent
date: 2026-06-23
tags: []
related:
- implements: RFC-048
---

## Changes

Two bugs under RFC-048 status model. PLAN ONLY.

Verified paths:
- `src/engine/config.rs` — `Lifecycle::has_edge` (~162), `Lifecycle::targets_from` (~171). Empty `edges` → both return nothing → every transition rejected.
- `src/cli/update.rs` — status guard (~31) calls `has_edge`.
- `src/tui/state/app.rs` — hardcoded 7-status lists: `cycle_filter_value_next` (~1815), `cycle_filter_value_prev` (~1848), `open_status_picker` index lookup (~2643), `confirm_status_change` index→status (~2667).
- `src/tui/views/overlays.rs` — `draw_status_picker` hardcoded list (~507).
- `src/tui/state/forms.rs` — `StatusPicker` struct (~218): `active`, `selected`, `doc_path`.

1. **Empty edges = any transition** (config.rs `has_edge`/`targets_from`).
   - `has_edge`: if `self.edges.is_empty()` → return `true` (any from→to ok). Else existing edge match.
   - `targets_from`: if `self.edges.is_empty()` → return all `states` except `from`. Else existing.
   - Edges already `#[serde(default)]` → optional in TOML. No schema change.
   - AC1, AC2.
   - Verify: type w/ states + no edges → `update --status` any→any ok; type w/ edges → off-edge rejected.

2. **StatusPicker carries config states** (forms.rs + app.rs).
   - Add `pub states: Vec<String>` to `StatusPicker`. Populate in `open_status_picker` from selected doc's type lifecycle (`config.type_by_name(doc.type).lifecycle.states`). `open_status_picker` needs `config` arg — thread it from caller.
   - `selected` index = position of `doc.status` in that `states` (fallback 0).
   - `confirm_status_change`: `status = Status::new(&self.status_picker.states[selected])`. Drop hardcoded match.
   - AC3.

3. **Status picker render reads picker states** (overlays.rs `draw_status_picker`).
   - Replace hardcoded `statuses` array w/ `app.status_picker.states`.
   - AC3.

4. **Filter cycling spans configured statuses** (app.rs `cycle_filter_value_next`/`_prev`).
   - Build status universe = union of all `config.documents.types[].lifecycle.states`, dedup, stable order (first-seen across types). Store on App at load (e.g. `all_statuses: Vec<String>`) or compute via config held by App.
   - next/prev = index into that vec, `None`→first/last, wrap to `None` at ends (keep current None-bracketing behaviour).
   - AC4.
   - Verify: custom config statuses appear in filter cycle; defaults unchanged for default config.

## Test Plan

- **AC1 — empty edges allow any.** `Lifecycle{states:[a,b,c], edges:[]}`: `has_edge(a,c)` true, `targets_from(a)` = [b,c].
- **AC2 — declared edges still gate.** default_lifecycle: `has_edge("draft","complete")` false; `update --status` off-edge bails.
- **AC3 — picker uses doc type states.** Custom type w/ states [x,y]: `open_status_picker` sets `states==[x,y]`, render lists x/y, confirm writes chosen.
- **AC4 — filter cycle uses config union.** Config w/ extra status `parked`: cycle_next reaches `parked`.

## Notes

- Edges optional was the RFC intent ("unconstrained any→any" baseline); empty-edges semantics make that explicit. Wildcard `*` source unchanged.
- TUI previously assumed lazyspec's 7 statuses everywhere; this removes the last hardcoded copies so custom DAGs render correctly.

