---
title: 'TUI rethink: one list, many scopes, a command palette'
type: rfc
status: draft
author: Jack Kaloger
date: 2026-09-08
tags: []
related:
- related-to: RFC-005
- related-to: RFC-047
- related-to: RFC-022
- related-to: STORY-032
- related-to: STORY-033
- related-to: AUDIT-018
- related-to: BUG-009
- related-to: STORY-035
- related-to: STORY-130
---

## Summary

Rebuild the TUI shell around three ideas: one document list with many scopes, a focus model with an overlay stack, and a command registry that drives the palette, the help overlay, and the README from one table. Types, Filters and Graph stop being modes and become ways of scoping and arranging the same list. Every CLI verb becomes reachable from the palette through `engine::ops`, so the TUI stops lagging the CLI and stops calling into it. Layout gets fixed chrome, proportional content, and three width breakpoints. Rendering code (`content/`, status bar, table drawing) stays; the shell (`App`, `keys.rs`, `views.rs` dispatch, `ViewMode`) is replaced.

## Motivation

1. **Navigation has no model.** `ViewMode` is a flat enum cycled by backtick (`state/app.rs:509`), with 24 `KeyContext` variants layered on as booleans and `Option`s checked in a fixed ladder (`views/keys.rs:19-81`). Return paths are ad hoc: `Enter` in Graph jumps to Types and forgets where you were; there is no way back. Twenty RFCs each added a mode or a flag; none added a stack.
2. **Global keys drift because each handler re-implements them.** Graph is missing `/`, `w`, `s`, `r`, `p`, `n`, `o`, `x`; Fullscreen is missing all of them. `o` means open externally in Types and cycle sort in Graph. `w` means warnings everywhere except Settings, where it saves. `d` is delete, delete entry, and discard. Search and the link editor are arrow-only while every other list takes `j/k`.
3. **Three modes are one list.** Types, Filters and Graph share the same 20/80 geometry and the same table drawer. They differ only in what sits in the left pane and whether the DOC column is a tree. Keeping them as modes triples the keymap and the tests for no extra capability.
4. **The TUI lags the CLI and reaches into it.** No TUI path for `unlink`, `tag`, `pin`, `ignore`, `why`, `context`. Meanwhile `state/app.rs:2450,2503,2528,2554,2613,2894,3173` and `event_loop.rs:902` call `crate::cli::*` directly, which the convention's third principle forbids and AUDIT-018 F2 already flagged.
5. **Layout is percentages all the way down.** Sidebar is always 20% even at 300 columns; the doc table always gets 40% of height even at 24 rows. On a wide terminal the preview sits under the list and wastes half the width. Nothing adapts below 80 columns.
6. **`App` is 11.3k lines with ~120 fields.** Focus is booleans, viewport heights are written back by the drawer each frame, every dialog struct is inline. AUDIT-008 split the directories but never the struct. New features go in by adding a field and an `if`.

## Goals

- One dispatch ladder: top overlay, then focused pane, then global. A key bound globally works in every pane with no per-pane code.
- `Esc` always means back: pop overlay, clear filter, unzoom, or step back in navigation history, in that order.
- Types, Filters and Graph replaced by sidebar scopes plus a flat/tree toggle on one list. Filtering is live from `/` and shows match highlights (absorbs STORY-130).
- Every CLI verb that applies to a document is reachable by name from `:`; `unlink`, `tag`, `pin`, `ignore`, `why`, `context` gain a TUI path with no new key.
- One `Command` table drives the palette, the `?` overlay (mode-aware, absorbs STORY-032), and the README keymap. A test asserts the README table matches.
- `src/tui` has zero `crate::cli` references.
- Layout has fixed-width chrome and three breakpoints; the TUI is usable at 80x24 and does not waste width at 200 columns.
- One `Theme` struct; no `Color::` literals outside it and `StatusPalette`.
- Layout snapshot tests at each breakpoint.
- Existing behaviour preserved: file watching, config hot reload, background fetch, staleness bands, inline diagrams, settings editor, agent screen behind its feature.

## Non-goals

- Multi-document selection and batch dispatch (RFC-047). The shell reserves `m` for marking so RFC-047 can land on top.
- User-remappable keys. The registry makes it a config table later; not now.
- Mouse support.
- Metrics mode. The stub pane is deleted; STORY-014 re-proposes against the new shell if wanted.
- Changes to the web view or CLI beyond moving TUI-used ops into `engine::ops`.
- Rewriting `content/` (GFM, diagrams, images) or the status bar. They render into a `Rect` and do not care who owns it.

## Design

### Screen

One screen, three panes, one command line. Settings and Agents are separate screens reached by name.

```
┌ lazyspec  ◐ synced 4s ago                              main ● 3 changed ┐
│ SCOPES      │ ID       DOC                       STATUS    TAGS  @      │
│ ▸ Types     │ ▾ RFC-072 External git repo…       in-prog   store       │
│   rfcs  71  │   STORY-281 Read a type's docs…    accepted              │
│ ▸ specs  22 │     ITER-433 Declare store git…    complete              │
│   ...       │   STORY-283 Resolve every type…    complete              │
│ Problems  4 │ ▸ RFC-071 Mechanical agent conf…   review                │
│ Stale     9 │                                                          │
│ Mine      3 ├──────────────────────────────────────────────────────────┤
│ Recent   12 │ Body │ Relations │ Checks │ Meta        fresh · 2 refs   │
│─────────────│ ## Summary                                               │
│ FILTERS     │ Let a type's documents live outside this repo…           │
│ status: any │                                                          │
│ tag: any    │                                                          │
│ view: tree  │                                                          │
├─────────────┴──────────────────────────────────────────────────────────┤
│ :                                                                      │
│ 1 scopes  2 docs  3 detail   ?  help   :  command   /  filter    ⏵ q   │
└────────────────────────────────────────────────────────────────────────┘
```

**Sidebar (1)** holds scopes and filters. Scopes are saved queries over the store: each type, plus built-ins `Problems` (docs with findings), `Stale` (band ≠ fresh), `Mine` (assignee = git user), `Recent` (git status ≠ clean). `Enter` applies a scope. Below, filter chips for status, tag, and view (flat/tree); `Space` cycles a chip, `c` clears. The graph pivot on a type or tag is exactly "scope = type, view = tree", so Graph needs no mode.

**List (2)** is today's documents table with the tree column from Graph folded in. Flat view is the current table; tree view is the current forest, rooted at the scope. Columns come from `[tui.table]` as now. `/` starts a live fuzzy filter on the list with match highlights; `Enter` keeps it, `Esc` clears it. `Space` expands and collapses. Sorting is `:sort <col>`.

**Detail (3)** has four tabs switched with `[` and `]`: Body (today's preview, staleness band in the tab bar), Relations (today's Relations tab, `Enter` follows and pushes history, `-` or `Backspace` goes back), Checks (validation findings for the selected doc; the global warnings panel becomes the `Problems` scope), Meta (frontmatter fields, attributes, provenance). `z` zooms the focused pane to full screen; `Esc` unzooms. Fullscreen reader is `z` on Detail.

**Command line** is one row that is the `:` palette input, the `/` filter input, and the toast/error line when neither is active. The full-screen search overlay goes away.

**Status bar** stays as configured under `[tui.statusbar]`. Its default right zone shows the focused pane's number and the four global keys.

### Layout

Chrome is fixed: header 1 row, command line 1 row, status bar 1 row, sidebar 22 columns. Content is proportional. Three breakpoints on width:

| Width | Sidebar | List / Detail |
|---|---|---|
| < 80 | hidden; `1` shows it as an overlay | one pane at a time; `Enter` on a doc zooms Detail |
| 80–139 | shown | Detail below List, 40/60 by height (today) |
| ≥ 140 | shown | Detail right of List, 50/50 by width |

`[tui.layout] detail = "auto" | "below" | "right"` overrides the last two. Heights the drawer measures are returned from `draw`, not written into state.

### Focus and overlays

```rust
enum Focus { Sidebar, List, Detail }

enum Overlay {
    Help, Palette(PaletteState), Confirm(Confirm), Create(CreateForm),
    Picker(Picker), Link(LinkEditor), Provenance(ProvenanceForm), Agent(AgentDialog),
}

struct Ui { focus: Focus, overlays: Vec<Overlay>, zoom: bool, history: Vec<DocId>, filter: Option<Filter> }
```

Key dispatch is one function: `overlays.last_mut()` handles first; if it returns `Fallthrough`, the pane under `focus` handles; if that falls through, the global table handles. Each handler returns `Handled | Fallthrough`. `Esc` is handled at each level in turn: pop overlay, clear filter, unzoom, pop history. Settings and Agents are `Screen`s that replace the three-pane body wholesale and have their own `Focus`; they keep their existing key tables, with `w`-to-save renamed to `Ctrl-S` only.

### Commands

```rust
struct Command {
    id: &'static str,          // "status", "link", "unlink", "tag", "pin", ...
    title: &'static str,
    key: Option<KeyEvent>,     // shortcut, if any
    scope: Scope,              // Global | Pane(Focus) | Doc (needs a selected doc)
    run: fn(&mut App, Args) -> anyhow::Result<Outcome>,
}
```

One `const COMMANDS: &[Command]`. The palette fuzzy-matches `id` and `title` over commands whose `scope` applies. `?` lists the same commands grouped by scope, so help is mode-aware by construction. A test renders the README keymap table from `COMMANDS` and diffs it against `README.md`. This replaces `views/keybinds.rs` and its parity test.

Commands call `engine::ops` only. The ops that today live in `cli/` and are called from `state/app.rs` move to `engine::ops` first; the CLI keeps its formatting and calls the same function.

### Keys

Global: `q` `Ctrl-c` quit · `?` help · `:` palette · `/` filter · `1` `2` `3` focus pane · `Tab` `Shift-Tab` next/prev pane · `z` zoom · `Esc` back · `,` settings.

Every list: `j` `k` arrows · `g` `G` · `Ctrl-d` `Ctrl-u` `PgDn` `PgUp` · `Space` expand or toggle chip.

List pane: `Enter` open detail · `n` new · `e` edit · `o` open externally · `d` delete · `s` status · `r` relations editor (add and remove) · `t` tags · `m` reserved (RFC-047).

Detail pane: `[` `]` tab · `Enter` follow relation · `-` back · `x` wrap.

Dropped: backtick cycle, `5`, `w`, `p` (palette `:provenance`), `R` (palette `:reload`; hot reload already automatic), `a` (palette `:agent`), `O` and graph `o` (palette `:sort`).

### State

`App` splits along what changes together:

- `Workspace`: store, seams (`fs`, `git`), caches, worker channels. Engine-facing, no UI.
- `Ui`: as above.
- `SidebarState`, `ListState`, `DetailState`: selection and scroll per pane, each with `handle_key` and `draw`.
- `Screen::{Main, Settings(SettingsState), Agents(AgentState)}`.

`Pane` is a trait with three implementations, which clears the bar for indirection. `Overlay` is an enum, not a trait; each variant is a struct that already exists in `state/forms.rs`.

### Theme

```rust
struct Theme { accent, border, muted, selection, danger, ok, warn: Color }
```

Loaded from `[tui.theme]` with today's colours as defaults. `StatusPalette` stays and reads `Theme` for its fallbacks. `views/colors.rs` becomes the only file that names a `Color`.

### Refresh

Unchanged: input thread, `notify` watcher, background poll, generation-stamped workers. They post `AppEvent`s to `Workspace`; `Ui` reads derived state. BUG-009 (rows go stale on external change) is fixed by the list deriving rows from `Workspace` on every frame it is dirty, rather than holding a copy.

### Tests

State tests keep driving `handle_key` and asserting on `Ui` and pane state. Layout gets `TestBackend` snapshots at 60x20, 100x30, 180x45 via `insta`, one per screen and zoom state. `surface_parity_test.rs` is unchanged and remains the gate that the tree view and the CLI `context` walk agree.

## Interfaces

All @draft.

- `tui::ui::{Focus, Overlay, Ui, Screen}`
- `tui::pane::Pane { fn handle_key(&mut self, key, ws: &mut Workspace) -> Handled; fn draw(&self, f, area, ws, theme) -> Measured }`
- `tui::command::{Command, Scope, COMMANDS, run(id, args)}`
- `tui::layout::{Breakpoint, Slots, compute(area: Rect, prefs: &LayoutPrefs) -> Slots}`
- `tui::theme::Theme`, config `[tui.theme]`, `[tui.layout] detail`
- `engine::ops::{unlink, tag, pin, ignore, unignore, why, context}` callable without `cli`
- Removed: `ViewMode`, `KeyContext`, `views/keybinds.rs`, `views/keys.rs`, `search_mode`, `show_warnings`, `fullscreen_doc`, graph anchor/sort fields, filters view state.

## Decisions (ADRs to emit)

1. Types, Filters and Graph are scopes and a view toggle on one list, not modes. Supersedes the mode model in RFC-005.
2. Key dispatch is one ladder: overlay stack, focused pane, global table. Panes never bind global keys.
3. A single `Command` table is the source of truth for palette, help, and README keymap. Single-letter keys are shortcuts into it.
4. TUI actions call `engine::ops` only; any op the TUI needs that lives in `cli/` moves to the engine first.
5. Layout is fixed chrome plus three width breakpoints, overridable by `[tui.layout]`.
6. Layout is snapshot-tested with `insta` at each breakpoint.

## Stories

Sequenced; each is shippable and leaves the TUI working.

1. **Ops out of `cli/`.** Move every op the TUI calls into `engine::ops`; CLI wraps them. Closes AUDIT-018 F2. No UI change.
2. **Shell.** `Focus`, `Overlay` stack, dispatch ladder, `Theme`, `layout::compute` with breakpoints. Existing panes ported into slots unchanged; existing overlays wrapped as `Overlay` variants. Backtick still cycles for now. Snapshot tests land here.
3. **Command table.** `COMMANDS`, `:` palette, `?` from the table, README table test. Delete `keybinds.rs`. `unlink`, `tag`, `pin`, `ignore`, `why`, `context` reachable via palette.
4. **Sidebar scopes and live filter.** Scopes replace the Types pane and Filters mode; `/` replaces the search overlay (absorbs STORY-130). Delete Filters mode.
5. **Tree toggle.** Graph's forest becomes `view: tree` on the list; delete Graph mode and backtick.
6. **Detail tabs and history.** Body, Relations, Checks, Meta; `[`/`]`; follow and back. Delete warnings overlay and fullscreen flag (now `z`).
7. **Decompose `App`.** `Workspace` plus per-pane state; retire render-fed heights. Fixes BUG-009.
8. **Docs.** Rewrite README TUI section from the table; supersede SPEC-015, 020, 021, 022; amend SPEC-014, 016, 017; supersede STORY-032, 033, 035.

Story 1 blocks 3. Story 2 blocks everything after it. Stories 4, 5, 6 are independent once 2 and 3 land. Story 7 can start after 2 and is best done last so port churn is paid once.

## Risks and tradeoffs

- **Test churn.** ~7k lines of `App` unit tests and 26 integration files assert on fields that will move. Porting pane by pane keeps each story's test delta bounded; some tests are deleted with the modes they cover.
- **Muscle memory.** Backtick, `5`, `w`, `p`, `R` go away. `?` and `:` make every dropped key discoverable, and the palette accepts the old names (`:warnings` → Problems scope).
- **Breakpoints surprise.** A user at 139 columns sees a different layout from one at 140. `[tui.layout] detail` pins it.
- **Scope of rewrite.** About 13k non-test lines are replaced; `content/`, `infra/`, status bar, and settings/agents screens are kept. If story 2 shows the settings screen fights the new `Focus`, it stays a `Screen` with its own tables rather than being forced into panes.
- **New dependency.** `insta` for snapshots. Accepted: it is the ecosystem norm and `cargo insta review` is what makes layout snapshots maintainable. Fallback is `TestBackend` plus fixture files if the dependency is refused.
- **Palette without keys is slower for frequent ops.** Kept single-letter keys for the eight most common document actions; everything else was already rare enough that no one filed a story for it.
