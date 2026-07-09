---
title: Render ClickUp-derived status colours in TUI/CLI/web
type: iteration
status: complete
author: jkaloger
date: 2026-07-09
tags: []
related:
- implements: STORY-201
---## Changes

Resolver (assumed, ITERATION-283): `status_colors::color_for(root: &Path, type_name: &str, status: &str) -> Option<String>` → `Some("#rrggbb")` on cache hit, `None` else. This slice CONSUMES it. Signature confirmed by ITERATION-283 (see Notes).

Renderers today take only `&Status` → must thread `type_name` (`doc.doc_type.as_str()`) + repo root (`store.root()`, `src/engine/store.rs:303`) to reach resolver. No engine change here; renderers ONLY.

### TUI — `src/tui/views/colors.rs`

- New fn `hex_to_color(hex: &str) -> Option<Color>` → parse `#rrggbb` → `ratatui::style::Color::Rgb(r,g,b)`; `None` on bad len/non-hex. Presentation-layer helper (principle 3).
- `status_color` (line 5): sig → `status_color(root: &Path, type_name: &str, status: &Status) -> Color`. Body → `color_for(root, type_name, status.as_str()).as_deref().and_then(hex_to_color).unwrap_or_else(|| match status.as_str() { ... })`. Existing hardcoded match (lines 6-15) = fallback arm, UNCHANGED.

Call sites thread `root`+`type_name`:
- `src/tui/views/panels.rs`
  - `doc_row_cells` (fn line 513, `status_color` call line 554): add params `type_name: &str, root: &Path`.
  - `doc_row_cells_expanded` (fn line 610): add same params; forward at inner `doc_row_cells` call (line 624).
  - Callers: line 712 (`doc_row_cells_expanded`), 725 + 1335 (`doc_row_cells`) → pass `doc.doc_type.as_str()` + `store.root()` (doc/store in scope at each). Test helpers `doc_row_cells_for_test` (2692), `doc_row_cells_gh_for_test` (2708) → add params, feed a fixture type + tmp root.
  - Metadata panel, line 931: `status_color(root, doc.doc_type.as_str(), &doc.status)` — `doc` + store in scope.
  - Line 1090: `status_color(root, target_doc.doc_type.as_str(), &target_doc.status)`. Else-branch missing-doc (line 1098) keeps `Color::Red`.
- `src/tui/views/overlays.rs`
  - `draw_status_picker` (line 525): resolve target doc via `app.store.get(&app.status_picker.doc_path)` (`StatusPicker.doc_path`, `src/tui/state/forms.rs:221`) → `type_name`; `root = app.store.root()`. Call `status_color(root, type_name, status)`.
  - Search overlay, line 956: `status_color(root, d.doc_type.as_str(), &d.status)`.

### CLI — `src/cli/style.rs`

- New fn `hex_to_ansi256(hex: &str) -> Option<u8>` → parse `#rrggbb` → nearest ANSI-256 index (6x6x6 cube + grayscale ramp). console 0.15 `Color` has NO truecolor variant (only `Color256(u8)`) → 256 is the ONLY faithful path; "truecolor" unreachable via `console::Style`. Flag in Notes.
- `status_style` (line 7): sig → `status_style(root: &Path, type_name: &str, status: &Status) -> Style`. Body → `hex_to_ansi256(color_for(...))` → `Style::new().color256(n)`; else existing match (lines 9-18) UNCHANGED.
- `styled_status` (line 21): sig → `styled_status(root, type_name, status)`; forwards.
- `doc_card` (line 47): add `root: &Path` param (already has `doc_type`); forward to `styled_status` at line 53.

Call sites (all have `store` + doc in scope → `store.root()`, `doc.doc_type.as_str()`):
- `src/cli/list.rs:23`, `src/cli/status.rs:77`, `src/cli/search.rs:42` (`doc_card`).
- `src/cli/show.rs:117` (`styled_status`).
- `src/cli/context.rs`: `styled_status` at 119, 265, 326, 350 → thread root + `child.doc_type`/`f.doc.doc_type`/`rel.doc.doc_type`. Render fns `render_stack`(136)/`render_tree`(154)/`push_card_children`(250)/`run_human`(303) already carry `store`.

### Web — `src/web/render.rs` + templates + `static/lazyspec.css`

ClickUp statuses match no `[data-status="..."]` selector (CSS keyed on default names, `static/lazyspec.css:300-348`) → currently no swatch colour. Emit inline CSS var from render.rs; CSS/inline mapping lives in web layer, engine stays CSS-ignorant (principle 3).

- Add field `status_color: Option<String>` to view models, filled via `color_for(store.root(), <type>, &status)`:
  - `DocRow` (line 23): built in `src/web/routes.rs` `build_groups` (line 69, type = `doc.doc_type` group key) + search map (line 278, `r.doc.doc_type`). `store.root()` in scope.
  - `ContextEntry` (line 126): built in `DocPage::from_doc` `entry` closure (line 207); has `d.doc_type` + `store`.
  - `DocPage` (line 138, own status line 230): use `doc.doc_type` + `store.root()`.
  - `GraphTreeNode` (line 274): built in `nest` (line 307) from `GraphNode` (`node.doc_type`, line 310). `nest` has no root → add `root: &Path` param; caller in `src/web/routes.rs::graph` (line 302) passes `store.root()`.
- Templates emit swatch inline style when `Some`, else current data-status behaviour:
  - `templates/list_row.html:1`, `templates/graph_node.html:4`, `templates/doc_page.html:23,41,47,53` — swatch `<span class="status-swatch">` → add `{% if let Some(c) = ...status_color %}style="--status-color:{{ c }}"{% endif %}` on the swatch span.
- `static/lazyspec.css`: `.status-swatch` (line 290) → `background: var(--status-color, var(--ink-faint));` so inline var wins, name-selector rules (300-348) remain fallback when var absent.

## Test Plan

Parse helpers (unit, no I/O):
- `src/tui/views/colors.rs` mod `tests`: `hex_to_color` → `#ff0000`→`Rgb(255,0,0)`; garbage (`#xyz`, `zzz`, `""`, `#ff00`) → `None`.
- `src/cli/style.rs` mod `tests`: `hex_to_ansi256` → known hex maps to expected cube index; pure black/white → ramp ends; garbage → `None`.

Resolver-hit renders derived hex (AC: TUI/CLI/web render ClickUp hex):
- TUI: fake resolver hit → `status_color(root, "clickup-tasks", &Status::new("pending"))` == `Color::Rgb(...)` matching cached hex. Extend `doc_row_cells_for_test` assertion (panels.rs `tests`, ~2780) — status cell fg = derived Rgb.
- CLI: `status_style(root,"clickup-tasks",&status)` yields `color256` of nearest index; assert style non-default.
- web: `render.rs` mod `tests` — build `DocRow`/`ContextEntry`/`GraphTreeNode` for a doc whose type has a cached colour → `status_color == Some("#rrggbb")`.

Fallback on miss (AC: non-ClickUp doc unchanged; missing colour → default, no crash):
- TUI: `status_color(root,"story",&Status::new("draft"))` (no cache) == `Color::Yellow` (existing match). Empty/absent cache root → same.
- CLI: `status_style(...,"story",draft)` == existing `.yellow()` path.
- web: no-cache doc → `status_color == None` → template omits inline var, data-status path intact.
- Garbage hex in cache → resolver returns `Some` but `hex_to_color`/`hex_to_ansi256` → `None` → falls to name match; no panic. Cover with a "cache holds `#zzzzzz`" case per TUI+CLI.

## Notes

- BLOCKED BY ITERATION-283 (engine: `color` on `ClickupStatus`, `.lazyspec/status-colors.json` cache mirroring `TaskMap` at `src/engine/task_map.rs`, resolver). Exact resolver name/sig from 283 — 283 ships `StatusColors::get(&self, type_name, status) -> Option<&str>` (a method on the loaded cache), NOT a free `color_for`. Adjust: each renderer loads `StatusColors::load(root)` once per render pass and calls `.get(type_name, status)`, or add a thin free `color_for(root,..)` wrapper in 283. Reconcile with 283's actual API at build time.
- Principle 3: CLI ⊥ TUI (no cross-dep). hex-parse duplicated per layer — TUI `hex_to_color`→`ratatui::Color::Rgb`, CLI `hex_to_ansi256`→`u8`; web needs no parse (hex → CSS var verbatim). Small dup accepted over shared presentation crate; engine never owns hex→Color.
- console 0.15 `Color` = named + `Color256(u8)` only, NO 24-bit. CLI "truecolor" is unreachable via `console::Style` → nearest-256 is the mechanism, not a fallback. TUI (ratatui) DOES emit true `Color::Rgb`. Downgrade fidelity beyond nearest-256 is out of scope (story).
- type_name source everywhere = `DocType::as_str()` (`src/engine/document.rs:69`); newtype `DocType(String)`. root = `Store::root()` (`src/engine/store.rs:303`). TUI reaches it via `app.store.root()`.
- Signature churn is intentional & in-scope: threading `type_name`+`root` through `doc_row_cells`/`doc_row_cells_expanded`/`status_color`/`status_style`/`styled_status`/`doc_card`/`GraphTreeNode::nest` + all listed call sites incl. test helpers. Adding params, not behaviour, on the fallback path.
- **Perf watch**: loading `StatusColors` from disk on every row render (list/graph can be many rows) → load once per render pass and pass the loaded cache down, not per-cell `load`. Confirm approach against 283's API at build.
- Parity (CLAUDE.md): all three surfaces in one slice — no surface ships without the others.
- YAGNI (principle 6): no colour-authoring UI, no config colours, no cache writes — read-only consumption of 283's cache.