---
title: TUI Settings Screen
type: rfc
status: draft
author: jkaloger
date: 2026-03-15
tags:
- tui
- config
- settings
related:
- related-to: RFC-013
- related-to: RFC-018
---


## Problem

Configuring lazyspec requires editing `.lazyspec.toml` by hand. Users need to know the config schema, find the file, open it in an editor, and get the TOML syntax right. There's no way to discover what's configurable without reading docs or source code.

This is friction that compounds. New users don't know config exists. Experienced users context-switch out of the TUI to tweak settings. Typos in TOML cause silent misbehavior (unknown keys are ignored).

## Intent

Add a settings view to the TUI that surfaces the current configuration and allows editing it in-place. The settings screen is a new view mode (accessible alongside Types, Filters, Metrics, Graph) that reads from and writes back to `.lazyspec.toml`.

The goal is discoverability first, editing second. Even a read-only settings view would be valuable, showing users what's configurable and what the current values are.

## Design

### View Mode

Settings becomes a new `ViewMode` variant, accessible via a number key (likely `5` or `0`). It follows the same flat navigation pattern as other modes.

@ref src/tui/state/app.rs#ViewMode

The settings screen has two panels:

```
┌─ Categories ────────┐ ┌─ Settings ──────────────────────────────┐
│  General             │ │  naming.pattern: "{type}-{n:03}-{title}"│
│  Document Types      │ │  ref_count_ceiling: 15                  │
│  Relationships       │ │  templates.dir: ".lazyspec/templates"   │
│  Validation Rules    │ │                                         │
│  Numbering           │ │                                         │
│  GitHub              │ │                                         │
│  Coordination        │ │                                         │
│  Certification       │ │                                         │
│  Agents              │ │                                         │
│▸ Interface           │ │  ── Interface ──                        │
│                      │ │  ascii_diagrams: false                  │
│                      │ │  statusbar.enabled: true                │
│                      │ │  multiline.max_expanded_height: 5       │
└──────────────────────┘ └─────────────────────────────────────────┘
```

Left panel: categories derived from config structure. Right panel: settings within the selected category, rendered as a form.

### Interaction model

Editing is **inline in the right panel**, not via modal overlays and not via an `$EDITOR` handoff. This is a new interaction pattern for the TUI: existing mutations either open an overlay (`CreateForm`, `StatusPicker`, `LinkEditor`) or suspend the TUI to run `$EDITOR` on a document file. Config editing instead operates directly on the focused field.

`j`/`k` move between fields, `Enter` starts editing the focused field, `Esc` cancels, `Enter` again confirms the field into the buffer. `Space` toggles bools and cycles enums without entering edit mode.

**Commit is atomic via a dirty buffer.** Confirmed edits accumulate in an in-memory `Config` buffer, not the file. A `dirty` indicator marks unsaved changes. `w` (or `Ctrl-S`) validates the *whole* buffer (including cross-field constraints) and, if valid, writes `.lazyspec.toml` once. `q`/`Esc` out of the view with unsaved changes prompts save/discard. The file never holds an invalid intermediate state, and a multi-field edit (e.g. switching a type to `sqids` numbering and supplying its salt) saves as one unit.

**Edits apply live after a successful save.** Today `Config` is loaded once at startup and threaded by clone through the TUI; the file watcher watches the document type dirs, *not* `.lazyspec.toml` (`@ref src/tui/infra/event_loop.rs`). So a config write does not currently propagate. The settings screen must make the session `Config` reloadable: on save it re-loads `Config`, rebuilds the `Store`, and re-establishes the watcher over the (possibly changed) type dirs, then redraws. `.lazyspec.toml` itself is also watched so an external change (e.g. `git pull`) reloads when the buffer is clean, or warns with keep/discard when the buffer is dirty. Config is not a lease-managed document; the lease system (RFC-018) applies to documents, not the config file.

### Categories

Categories map directly to `.lazyspec.toml` sections. The full config surface lives in `src/engine/config.rs#Config`:

| Category | Config Section | Fields |
|----------|---------------|--------|
| General | top-level | `naming.pattern`, `ref_count_ceiling`, `templates.dir` |
| Document Types | `[[types]]` | `name`, `plural`, `dir`, `prefix`, `icon`, `numbering`, `subdirectory`, `store`, `singleton`, `parent_type`, `agents` |
| Relationships | `[[relationships]]` | `name`, `inverse` |
| Validation Rules | `[[rules]]` | `parent-child` / `relation-existence` variants |
| Numbering | `[numbering.sqids]`, `[numbering.reserved]` | `salt`, `min_length` / `remote`, `format`, `max_retries` |
| GitHub | `[github]` | `repo`, `cache_ttl` |
| Coordination | `[coordination]` | `remote`, `lease_duration`, `grace_period`, `max_push_retries`, `max_clock_skew` |
| Certification | `[certification]` | `normalize`, `overrides` (per-spec map) |
| Agents | `[agents]` | `interactive` |
| Interface | `[tui]` | `ascii_diagrams`, `statusbar` (`enabled`, `left`, `center`, `right`), `multiline` (`max_expanded_height`) |

@ref src/engine/config.rs#Config

Each category renders the section's current values, falling back to the engine defaults where a section is absent (`[github]`, `[coordination]`, `[numbering.*]`, and `[agents]` are all optional). Optional sections that are unset render as `(unset)` rather than fabricating a section the user never wrote.

### Editing

Each field renders as an editable form element. The editor depends on the field type, which the settings screen derives from the `Config` struct:

| Field Type | Example | Editor |
|-----------|---------|--------|
| `String` | `naming.pattern`, `dir` | Inline text input |
| `bool` | `ascii_diagrams`, `certification.normalize` | Toggle (`Space` to flip) |
| `u8` / `usize` (bounded) | `sqids.min_length` (1-10), `ref_count_ceiling` | Numeric input, rejects out-of-range |
| `Option<String>` | `github.repo`, `parent_type`, `agents.interactive`, `inverse` | Nullable text input (empty = unset) |
| duration string | `lease_duration` (`60m`), `grace_period` (`2m`) | Text input, validated as a duration on save |
| `Vec<String>` | `agents`, `statusbar.left` | Comma-separated text input |
| `enum` | `numbering`, `store`, `format`, `severity`, rule `shape` | Cycle through variants with `Space` |

Enum variant sets are taken from the config enums: `numbering` is `incremental`/`sqids`/`reserved`, `store` is `filesystem`/`github-issues`/`git-ref`, reserved `format` is `incremental`/`sqids`, rule `severity` is `error`/`warning`, rule `shape` is `parent-child`/`relation-existence`.

Field-level editors reject malformed input as it is typed (numeric out of range, unparseable duration). The interaction mirrors the field-cycle and text-entry already in `CreateForm`.

@ref src/tui/state/forms.rs#CreateForm

### Collection sections (Types, Relationships, Rules, Overrides)

`[[types]]`, `[[relationships]]`, and `[[rules]]` are arrays of structs; `[certification.overrides]` is a keyed map (spec path to `{ normalize }`). All four render as an entry list within their category.

Navigation is **drill-in**: `j`/`k` move between entries, `Enter` on an entry replaces the right panel with that entry's field list (a breadcrumb such as `Document Types > rfc`), and `Esc` returns to the entry list. This keeps an 11-field `[[types]]` entry readable by editing one entry at a time.

Adding and removing entries:
- `n` inserts a default-seeded entry (a `[[types]]` entry from the `starter_types` shape, a rule defaulting to `parent-child`) and immediately drills into it so the user fills the fields
- `d` deletes the selected entry (with confirmation, following the delete pattern from RFC-004)

`[[relationships]]` is editable like any other collection, but `d` on the last entry is refused: a config with no `[[relationships]]` is a hard load error (ADR-011), so the buffer guards against producing one.

### Dependency auto-scaffolding

Some edits create a dependency on another section. When a type's `numbering` is cycled to `sqids` and the buffer has no `[numbering.sqids]`, the buffer auto-inserts a default `[numbering.sqids]` (empty `salt`, `min_length = 3`), marks the required-but-empty `salt`, and offers to jump focus to it. The same applies to `numbering = "reserved"` (inserts `[numbering.reserved]`) and `store = "github-issues"` (inserts `[github]`). Save-time validation still enforces the constraint (a `sqids` salt must be non-empty), so auto-scaffolding guides the user without weakening validation.

### Validation and write-back

Save runs two layers of validation against the buffer before any write:

1. Field-level: already enforced as values are entered (range, parseability, enum membership).
2. Config-level cross-field constraints, mirroring `Config::parse`, surfaced as a footer error that blocks the save and jumps focus to the offending field:
   - A type with `numbering = "sqids"` requires `[numbering.sqids]` with a non-empty `salt` and `min_length` in `[1, 10]`.
   - A type with `numbering = "reserved"` requires `[numbering.reserved]`; if its `format = "sqids"`, the sqids salt is also required.
   - A type with `store = "github-issues"` requires a `[github]` section.
   - `[[relationships]]` must be non-empty.
   - Rule `child` / `parent` / `type` should name a declared `[[types]]` entry.

A valid buffer is written with `toml_edit` (preserving the file's existing formatting and comments) rather than serializing the `Config` struct. If `.lazyspec.toml` doesn't exist, the first save creates it, seeding `[[types]]` and `[[relationships]]` so the result is loadable.

Some type-field changes have consequences for documents already on disk. When a save changes a type's `dir`, `prefix`, or `store` and that type has existing documents, the save pauses and lists the affected documents (e.g. "12 docs in `docs/rfcs` will no longer be found; files are not moved"), requiring explicit confirmation. The settings screen edits config only; it does not move, rename, or renumber documents. After a confirmed write, validation runs and any new document warnings/errors show inline.

@ref src/engine/config.rs#Config

## Stories

1. **Settings view mode and read-only display** -- New `ViewMode::Settings`, category navigation across all ten categories, drill-in into collection entries, read-only rendering of every config field including optional sections (shown as `(unset)`). No editing yet. Wired to a number key.

2. **Reloadable session config** -- Make the TUI `Config` reloadable rather than a startup constant: a save (initially a no-op re-load) re-loads `Config`, rebuilds the `Store`, and re-establishes the watcher over the type dirs; `.lazyspec.toml` is added to the watch set. This is the foundation any write depends on.

3. **Inline scalar editing with atomic save** -- Field editors for text, bool, numeric (bounded), nullable, duration, list, and enum-cycle. Dirty buffer, `w`/`Ctrl-S` whole-config validation, `toml_edit` write-back, save/discard prompt on exit, live reload after save.

4. **Dependency auto-scaffolding** -- On enum edits that create a section dependency (`numbering` -> sqids/reserved, `store` -> github-issues), insert the dependent section with defaults, mark required-but-empty fields, offer to jump focus. Save-time validation enforces the constraint.

5. **Collection management** -- Drill-in add/delete for `[[types]]`, `[[relationships]]`, `[[rules]]`, and `[certification.overrides]`: `n` seeds a default entry and drills in, `d` deletes with confirmation, the last `[[relationships]]` entry is protected.

6. **Document-impact guard** -- Detect saves that change a type's `dir`/`prefix`/`store` for a type with existing documents, list the affected documents, and require confirmation before writing.

7. **Interface settings integration** -- Once RFC-022 lands, the Interface category controls `[tui.statusbar]` component ordering (`left`/`center`/`right`) alongside `ascii_diagrams` and `[tui.multiline]`.
