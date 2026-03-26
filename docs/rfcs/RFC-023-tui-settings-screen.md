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
- related to: RFC-013
- related to: RFC-018
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

@ref src/tui/app.rs#ViewMode

The settings screen has two panels:

```
┌─ Categories ────────┐ ┌─ Settings ──────────────────────────────┐
│  General             │ │  docs_dir: "docs"                       │
│  Document Types      │ │  ascii_diagrams: false                  │
│▸ Validation Rules    │ │                                         │
│  Status Bar          │ │  ── Validation Rules ──                 │
│                      │ │  stories-need-rfcs                      │
│                      │ │    shape: parent-child                  │
│                      │ │    child: story                         │
│                      │ │    parent: rfc                          │
│                      │ │    severity: warning                    │
│                      │ │                                         │
└──────────────────────┘ └─────────────────────────────────────────┘
```

Left panel: categories derived from config structure (`[types]`, `[[rules]]`, `[tui]`, general top-level fields). Right panel: settings within the selected category, rendered as a form.

### Categories

Categories map directly to `.lazyspec.toml` sections:

| Category | Config Section | Fields |
|----------|---------------|--------|
| General | top-level | `docs_dir`, `ascii_diagrams` |
| Document Types | `[[types]]` | List of type definitions |
| Validation Rules | `[[rules]]` | List of rule definitions |
| Status Bar | `[tui.statusbar]` | `enabled`, `left`, `center`, `right` |

@ref src/engine/config.rs#Config

### Editing

Each field renders as an editable form element. The type of editor depends on the field type:

| Field Type | Editor |
|-----------|--------|
| `String` | Inline text input |
| `bool` | Toggle (`Space` to flip) |
| `Vec<String>` | Comma-separated text input |
| `enum` (e.g. severity) | Cycle through options with `Space` |

Navigation: `j`/`k` to move between fields, `Enter` to start editing a field, `Esc` to cancel, `Enter` again to confirm. This mirrors the create form interaction pattern from RFC-003.

@ref src/tui/app.rs#CreateForm

### Array Sections (Types, Rules)

`[[types]]` and `[[rules]]` are arrays of structs. The settings screen renders these as a sub-list within the category. `j`/`k` navigates between entries, `Enter` expands an entry to show its fields.

Adding and removing entries:
- `n` creates a new entry with defaults
- `d` deletes the selected entry (with confirmation, following the delete pattern from RFC-004)

### Write-back

Changes are written to `.lazyspec.toml` on confirm. The write-back uses `toml_edit` (which preserves formatting and comments) rather than serializing from the `Config` struct, to avoid destroying any manual formatting in the file.

If `.lazyspec.toml` doesn't exist, the settings screen shows defaults and creates the file on first edit.

The store reloads after config write via the existing file watcher, so changes take effect immediately across all views.

### Validation

After writing, run validation and show any new warnings/errors inline in the settings view. Invalid configurations (e.g. a type with no `dir`) are caught before writing and shown as an error message at the bottom of the form.

## Stories

1. **Settings view mode and read-only display** -- New `ViewMode::Settings`, category navigation, read-only rendering of all config fields. No editing yet. Wired to a number key.

2. **Inline config editing** -- Form editors for each field type (text, bool, enum, list). `Enter`/`Esc` edit cycle. `toml_edit`-based write-back that preserves formatting. Validation on save.

3. **Array section management** -- Add/remove entries for `[[types]]` and `[[rules]]`. Sub-list navigation. Delete confirmation. Default values for new entries.

4. **Status bar settings integration** -- Once RFC-022 lands, the Status Bar category in settings controls `[tui.statusbar]` config with component ordering.
