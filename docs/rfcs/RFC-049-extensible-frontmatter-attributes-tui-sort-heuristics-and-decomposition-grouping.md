---
title: "Extensible frontmatter attributes, TUI sort heuristics, and decomposition grouping"
type: rfc
status: accepted
author: "jkaloger"
date: 2026-06-23
tags: []
related: []
---<!-- intent: propose a design and the decisions it forces, before code -->

## Summary

Add typed, config-declared custom attributes to document frontmatter (with validation), then use them in the TUI graph view. The graph view gains a navigable left column that **pivots** the forest on a chosen anchor type, and renders the decomposition as a nested table whose columns are config-declared (status, attributes, cross-cutting relations). Sort orders siblings by any column.

## Motivation

Problems, in priority order:

1. **Graph is unpivotable.** The forest is one fixed shape (lexicographic roots, `implements` lineage). There is no way to say "anchor on RFC, show what lives under it" then re-pivot to "anchor on story". The DAG is richer than one rooting.
2. **No second axis.** Cross-cutting relations (`related-to`, ADR/req links) exist but are buried as inline annotations; they read as noise rather than a distinct lens.
3. **Sort is lexicographic by path only** (`graph.rs:55,80`). No useful heuristic (estimate, priority, status).
4. **Frontmatter is not extensible.** `RawFrontmatter` (`document.rs:197`) silently drops unknown keys. No estimate/priority exists to sort or display, and typos pass unvalidated.

These are one design: attributes are the foundation; pivot, columns, and sort all consume them.

## Goals

- Declare custom attributes per type in config, typed and validated.
- Validate frontmatter against the schema (`lazyspec validate`).
- Expose attributes in `--json` (`show`, `status`) so agents read them.
- Pivot/anchor the forest on a chosen type. Re-rooting is an **engine** `context` operation, exposed via `context --json --anchor <type>`, consumed identically by CLI and TUI (dictum 3, principle 2).
- Graph view: pick the anchor via a navigable left column, same interaction grammar as the types view; TUI only renders the engine-produced forest.
- Anchor extent = anchor docs as roots + their decomposition descendants nested.
- Render the forest as a nested table with config-declared columns (status, attributes, related).
- Sort siblings by any column; total, stable order (path tiebreaker).

## Non-goals

- Attribute-based *grouping* (group all docs with `phase=2`). Grouping is by decomposition/anchor type only.
- Free-form / global attributes. Attributes are per-type.
- Swapping the primary relation axis (re-rooting on `related-to`). Cross-cutting stays a column, not a tree.
- Column-header click-to-sort (needs header focus state the TUI lacks).

## Design

### Attribute schema (engine)

New `attributes: Vec<AttrDef>` field on `TypeDef` (`config.rs:195`). Each `AttrDef`:

- `name: String`
- `kind: AttrKind` — one of `int | float | string | enum | date | bool`
- `required: bool` (default false)
- `values: Vec<String>` (for `enum`)

Frontmatter capture: `RawFrontmatter` / `DocMeta` (`document.rs:182,197`) gains an `attributes: BTreeMap<String, AttrValue>` capturing declared keys. `date` reuses the existing custom deserializer (`document.rs:11`). Undeclared keys are still captured (so validate can warn) but are not typed.

### Validation

`lazyspec validate` checks each doc's attributes against its type's schema:

- wrong kind, or `enum` value not in `values` -> **error**
- missing `required` attribute -> **error**
- frontmatter key not declared on the type -> **warning** (catches typos, allows ad-hoc notes)

Slots beside existing rule severities (`config.rs` rules: error/warning).

### Anchoring (engine)

Re-rooting lives in the engine, not the TUI. `resolve_forest` (`context.rs:171`) gains an optional anchor: `resolve_forest(store, anchor: Option<&str>)`. When `anchor` is set:

- roots = all docs of the anchor type (instead of all DAG roots)
- below each root, its `implements`-descendant decomposition (the existing topo walk, pruned to descendants)
- docs above the anchor in the DAG are excluded

Lineage relation stays `implements` (hardcoded today); parameterising the lineage relation is a non-goal here. `ContextNode` is unchanged; anchoring only changes the root set + pruning.

### CLI exposure

`context --json` gains `--anchor <type>`, emitting the anchored forest. Agents get the same pivot the TUI does (principle 2). The whole-store forest remains the default when no anchor is passed.

### Graph pivot (TUI render only)

The graph view's dead left column (`views.rs:1555`, empty " Graph " block) becomes a **pivot picker**: a type list navigated `h`/`l` exactly like the types-view sidebar (`move_type_prev/next`, `selected_type` at `app.rs:441`). Selecting a type sets `graph_anchor` on `App`; `rebuild_graph` (`app.rs`) passes it to the engine `resolve_forest(store, anchor)`, then `flatten_forest` renders the result. No re-rooting logic in the TUI -- it consumes the engine forest as before.

### Nested table render (TUI)

Replace the connector-art-only render (`panels.rs:1555`) with a nested table, matching the types-view nested doc table. Columns come from config `tui.graph.columns` (default `["status", "related"]`). Valid column ids: `status`, `related`, and any attribute declared on a visible type. The DOC column keeps the tree indentation/connectors; other columns align right of it.

```
DOC         STATUS    PRI   EST   REL
● RFC-049   accepted
 ├ STORY-1  in-prog   high   5    ADR-12
 └ STORY-2  draft     low    2
```

### Sort (TUI)

A key cycles the active sort column over `path | status | <attributes>`; `tui.graph.sort` sets the default. Sort applies **within siblings** (preserves the tree), with `path` as final tiebreaker -> total, stable order. Header shows the active column + direction.

Key: `s` is taken (status picker). Proposed `o` to cycle, `O` to reverse. (CONFIRM)

## Interfaces

Proposed, `@draft`:

- `struct AttrDef { name, kind: AttrKind, required: bool, values: Vec<String> }` @draft
- `enum AttrKind { Int, Float, Str, Enum, Date, Bool }` @draft
- `enum AttrValue { ... }` @draft on `DocMeta`
- `TypeDef.attributes: Vec<AttrDef>` @draft
- `DocMeta.attributes: BTreeMap<String, AttrValue>` @draft
- config `tui.graph.columns: Vec<String>`, `tui.graph.sort: String` @draft
- `App.graph_anchor`, sort-state fields @draft
- engine `resolve_forest(store, anchor: Option<&str>)` gains the anchor param @draft
- CLI: `context --json --anchor <type>` @draft
- CLI: `show`/`status` `--json` emit `attributes`

## Decisions (ADRs to emit)

- Per-type attribute schema on `TypeDef` (vs global registry / free-form).
- Anchoring is an engine `context` operation exposed via `context --json` (vs re-rooting in the TUI). Keeps graph-shaping in the engine; agents and TUI share it.
- Pivot-by-anchor-type forest model (vs fixed rooting).
- Graph view as a config-column nested table (vs connector art only).

## Stories

Vertical slices, in dependency order:

1. **Engine: attribute schema + capture + validation.** `AttrDef`/`AttrKind`/`AttrValue`, `TypeDef.attributes`, frontmatter capture, validate rules. Foundation.
2. **Engine + CLI: anchored forest.** `resolve_forest(store, anchor)`, `context --json --anchor <type>`. Independent of attributes; shapes the graph.
3. **CLI: expose attributes in `--json`.** `show`/`status` emit `attributes`; `validate` surfaces schema findings. (depends on 1)
4. **TUI: pivot sidebar.** Type-picker left column, `graph_anchor`, `rebuild_graph` passes anchor to the engine. (depends on 2)
5. **TUI: nested table + config columns + sort.** Table render, `tui.graph.columns`, sort cycle key, `tui.graph.sort`. (depends on 1, 4)

## Risks and tradeoffs

- **Attribute capture changes the document model.** `DocMeta` is parsed widely; adding a map is additive but touches a hot struct. Accepted: keeps attributes first-class rather than a side table.
- **Validation severity for undeclared keys.** Warning, not error, so legitimate ad-hoc frontmatter survives. Cost: typos are warnings, not blocks.
- **Re-rooting cost.** Engine `resolve_forest` re-runs on every pivot. Forests are small (project-scale docs); accepted over caching.
- **Table width.** Many attribute columns overflow narrow terminals. Mitigated by config-declared (not auto-all) columns; default is just status+related.
- **Anchor hides ancestors.** Anchoring on a deep type hides parents. Accepted: matches the "what lives under X" intent; pivot to the parent type to see them.
