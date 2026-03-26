---
title: TUI Progressive Disclosure
type: rfc
status: accepted
author: jkaloger
date: 2026-03-05
tags:
- tui
- design
related:
- implements: RFC-001
---



## Problem

The TUI is a flat, single-mode browser. The type panel, document list, and preview are always visible and always show the same kind of information. As document counts grow (team usage, long-running projects), the dashboard lacks tools for understanding project health at a glance: which documents are stuck in draft, how flow through the lifecycle looks over time, and where broken links or orphaned documents exist.

Adding these capabilities to the current layout would overload it. Sparklines, filter controls, dependency graphs, and validation indicators competing for space in three fixed panels doesn't work. The TUI needs a way to show different information at different times without adding panels or tabs indefinitely.

## Intent

Introduce a **mode system** where the left panel determines what the entire screen shows. The left panel cycles between modes; the right side transforms to match. Each mode gets the full layout rather than competing for space within it.

This also introduces two inline interactions (status editing and opening `$EDITOR`) and passive validation indicators that work across modes.

## Modes

Four modes, cycled with backtick:

| Mode | Left panel | Right side |
|------|-----------|------------|
| **Types** | Type selector with counts (current behaviour) | Doc list + Preview/Relations tabs |
| **Filters** | Status, tag, author, sort controls | Filtered doc list + Preview/Relations tabs |
| **Metrics** | Per-type sparklines | Status flow chart + summary statistics |
| **Graph** | Legend and controls | Full navigable dependency tree |

A mode indicator in the title bar shows the current mode and hints at the cycle key.

### Types mode (default)

No changes from current behaviour. This is the home base.

```
  lazyspec                                    [Types] ` to cycle
┌─ Types ─────────────┐ ╔═ Documents ════════════════════════════════╗
│  RFCs        (5)    │ ║  RFC-001-core-tool        accepted [tui]  ║
│  ADRs        (3)    │ ║  RFC-002-ai-workflow       accepted [ai]  ║
│  Stories     (8)    │ ║! RFC-003-tui-creation      draft    [tui]  ║
│  Iterations  (12)   │ ╚════════════════════════════════════════════╝
│                     │ ┌─ Preview | Relations ──────────────────────┐
│                     │ │                                            │
│                     │ │  (document preview as current)             │
│                     │ │                                            │
└─────────────────────┘ └────────────────────────────────────────────┘
```

The `!` prefix on documents with validation errors is new. It appears in any mode that shows the doc list.

### Filters mode

The left panel becomes a form with filter/sort controls. The doc list on the right applies those filters. Preview and Relations tabs continue to work normally against the filtered list.

```
  lazyspec                                  [Filters] ` to cycle
┌─ Filters ───────────┐ ╔═ Documents (2 of 5) ══════════════════════╗
│                     │ ║  RFC-003-tui-creation      draft    [tui]  ║
│  Status: [draft   ] │ ║  RFC-006-progressive       draft    [tui]  ║
│  Tag:    [tui     ] │ ║                                            ║
│  Author: [all     ] │ ╚════════════════════════════════════════════╝
│  Sort:   [date   v] │ ┌─ Preview | Relations ──────────────────────┐
│                     │ │                                            │
│  [clear filters]    │ │  (preview of selected filtered doc)        │
│                     │ │                                            │
└─────────────────────┘ └────────────────────────────────────────────┘
```

Filter controls are navigated with `j/k` to select a field and `h/l` to cycle values within it. The doc list updates live as filters change. `Enter` on "clear filters" resets all filters. Filters persist when switching back to Types mode (a filter-active indicator shows in the title bar).

> [!NOTE]
> Filters apply to the **doc list across Types and Filters modes**. The type selector in Types mode acts as an additional implicit filter. In Filters mode, all types are shown unless a type filter is added.

### Metrics mode

The left panel shows per-type sparklines (document creation over time). The right side replaces the doc list and preview with a status flow chart and summary statistics.

```
  lazyspec                                  [Metrics] ` to cycle
┌─ Metrics ───────────┐ ┌─ Status Flow ─────────────────────────────┐
│                     │ │                                            │
│  RFCs    ▁▂▃▅▂▁    │ │  accepted  ▁▂▃▅▇▅▃▂                      │
│  ADRs    ▁▁▂▃▁▁    │ │  draft     ▇▅▃▂▁▁▁▁                      │
│  Stories ▁▁▂▅▇▃    │ │  review        ▁▂▃▂▁                      │
│  Iters   ▁▃▅▇▅▂    │ │  rejected  ▁▁▁▁▁▁▁▁                      │
│                     │ ├────────────────────────────────────────────┤
│                     │ │  Total: 28        This week: 4            │
│                     │ │  Oldest draft: 12d ago                    │
│                     │ │  ⚠ 2 broken links                         │
│                     │ │  ⚠ 1 unlinked iteration                   │
│                     │ └────────────────────────────────────────────┘
└─────────────────────┘
```

Left panel sparklines show document creation volume over time (one data point per week, based on the `date` frontmatter field). The right side shows status flow sparklines (how many documents were in each status per week) and summary statistics.

The validation summary at the bottom surfaces `Store::validate()` results as counts. This is a passive view; selecting a validation warning could later navigate to the offending document, but that's out of scope for this RFC.

Sparklines use ratatui's `Sparkline` widget. Each sparkline maps to a `Vec<u64>` of weekly counts derived from document dates.

### Graph mode

The left panel shows a legend and filter controls. The right side renders the full dependency graph as a navigable tree.

```
  lazyspec                                    [Graph] ` to cycle
┌─ Graph ─────────────┐ ┌─ Dependency Graph ────────────────────────┐
│                     │ │                                            │
│  Filter: [all]      │ │  RFC-001 Core Document Management Tool    │
│                     │ │   ├─▶ STORY-001 Document Model            │
│  Legend             │ │   │   ├─▶ ITER-001 Design                 │
│  ● RFC              │ │   │   └─▶ ITER-002 Implementation         │
│  ■ ADR              │ │   ├─▶ STORY-002 CLI Commands              │
│  ▲ Story            │ │   └─▶ STORY-003 TUI Dashboard             │
│  ◆ Iteration        │ │       ├─▶ ITER-004 Delete                 │
│                     │ │       └─▶ ITER-005 Flat Nav               │
│  Edges              │ │                                            │
│  ─▶ implements      │ │  RFC-002 AI-Driven Workflow               │
│  ─▷ blocks          │ │   └─▶ STORY-005 Agent Skills              │
│  ┄▷ related-to      │ │       └─▶ ITER-003 Submit                 │
│  ══▶ supersedes     │ │                                            │
│                     │ │                                            │
│  j/k  navigate      │ │                                            │
│  Enter jump to doc  │ │                                            │
└─────────────────────┘ └────────────────────────────────────────────┘
```

The graph is built by walking `Store::forward_links` from root documents (documents with no incoming `implements` links). Each node shows a type indicator, the document title, and its status colour. The currently selected node is highlighted with cyan/bold.

Navigation:
- `j/k` moves between nodes in depth-first order
- `Enter` jumps to that document in Types mode (switches mode, selects the type, selects the doc)
- `h/l` collapses/expands subtrees
- The filter on the left panel scopes the graph to a document type or status

> [!WARNING]
> The graph layout algorithm is the hardest part of this RFC. A simple tree rendering (indented lines with box-drawing characters) is sufficient for the `implements` hierarchy. Cross-cutting edges (`blocks`, `related-to`, `supersedes`) are harder to render inline without visual noise. The initial implementation should render the `implements` tree and list cross-cutting relations as annotations on nodes rather than drawn edges.

### Graph rendering strategy

Phase 1: tree rendering only. Walk `implements` edges to build a forest (multiple root nodes). Render as an indented tree with box-drawing characters. Cross-cutting edges (`blocks`, `related-to`, `supersedes`) appear as inline annotations:

```
  RFC-001 Core Document Management Tool
   ├─▶ STORY-001 Document Model
   │   └─▶ ITER-001 Design                    ┄▷ ADR-001
   ├─▶ STORY-002 CLI Commands
```

Phase 2 (future): canvas-based rendering with actual edge drawing for cross-cutting relations. This would use ratatui's `Canvas` widget for arbitrary line drawing. Out of scope for this RFC.

## Inline interactions

These work in Types and Filters modes (anywhere the doc list is visible).

### Status picker (`s`)

Pressing `s` on a selected document opens a small overlay listing all statuses. The current status is pre-selected. `j/k` navigates, `Enter` confirms, `Esc` cancels.

```
╔═ Documents ═══════════════════════════════╗
║ RFC-001  accepted [tui]                   ║
║ RFC-002  accep┌─ Status ───────┐          ║
║ RFC-003  draft│  draft         │          ║
║               │  review        │          ║
║               │> accepted      │          ║
║               │  rejected      │          ║
║               │  superseded    │          ║
║               └────────────────┘          ║
╚═══════════════════════════════════════════╝
```

On confirm, writes the new status to the document's frontmatter on disk (reusing the pattern from `cli::update`) and reloads the document in the store. The overlay follows the same visual conventions as the delete confirm dialog.

### Open in editor (`e`)

Pressing `e` on a selected document opens it in `$EDITOR` (falling back to `$VISUAL`, then `vi`). The TUI suspends (raw mode disabled, alternate screen exited), the editor runs in the foreground, and when it exits the TUI resumes and reloads the edited document.

This is the same terminal suspend/resume pattern used by tools like lazygit. crossterm provides `LeaveAlternateScreen` and `EnterAlternateScreen` for this.

## Validation indicators

`Store::validate()` already detects broken links, unlinked iterations, and unlinked ADRs. This RFC surfaces those results passively.

In the doc list, documents with validation errors show a `!` prefix before the filename:

```
  RFC-001-core-tool        accepted [tui]
! RFC-003-tui-creation      draft    [tui]
  RFC-005-flat-nav          accepted
```

Validation runs once on store load and refreshes when documents change. The results are cached on `App` as a `HashSet<PathBuf>` of documents with errors, checked during rendering.

In Metrics mode, validation error counts appear in the summary statistics panel.

## Mode switching

Backtick cycles through modes in order: Types -> Filters -> Metrics -> Graph -> Types. The current mode is stored as an enum on `App`:

@ref src/tui/app.rs#ViewMode@44b726a53d27437c23e35403bacd2d73b0054238

The `draw` function dispatches to a mode-specific renderer. Each mode owns its right-side layout entirely.

Filters mode has additional state (active filter values). These persist across mode switches so you can set filters, switch to Types mode, and still see them applied.

Graph mode has its own navigation state (selected node index, collapsed subtrees). This resets when entering Graph mode.

## State additions

```rust
// App struct additions
pub view_mode: ViewMode,
pub validation_errors: HashSet<PathBuf>,

// Filter state
pub filter_status: Option<Status>,
pub filter_tag: Option<String>,
pub filter_author: Option<String>,
pub sort_field: SortField,
pub sort_direction: SortDirection,

// Graph state
pub graph_nodes: Vec<GraphNode>,
pub graph_selected: usize,
pub graph_collapsed: HashSet<PathBuf>,

// Status picker
pub status_picker: Option<StatusPicker>,
```

## Stories

1. **Mode system and view switching** -- Add `ViewMode` enum, backtick cycling, mode indicator in title bar, dispatch to mode-specific renderers. Skeleton implementations for each mode (renders the panel borders and titles, no content yet).

2. **Filters mode** -- Filter state on `App`, filter controls panel with `j/k`/`h/l` navigation, filter application to doc list, "clear filters" action, filter persistence across mode switches, filtered count in doc list title.

3. **Metrics mode** -- Weekly count aggregation from document dates, sparkline rendering for per-type and per-status views, summary statistics panel, validation error counts.

4. **Graph mode** -- Tree construction from `implements` edges, depth-first node flattening, tree rendering with box-drawing characters, `j/k` navigation, `h/l` collapse/expand, `Enter` to jump to document, cross-cutting edge annotations.

5. **Status picker** -- `s` key handler, overlay rendering, `j/k`/`Enter`/`Esc` interaction, frontmatter write-back via `cli::update`, store reload.

6. **Open in editor** -- `e` key handler, terminal suspend/resume, `$EDITOR`/`$VISUAL`/`vi` fallback chain, document reload on editor exit.

7. **Validation indicators** -- Run `Store::validate()` on load, cache error paths as `HashSet<PathBuf>`, render `!` prefix in doc list, refresh on document changes.
