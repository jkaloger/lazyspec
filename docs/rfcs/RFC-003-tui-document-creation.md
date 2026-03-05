---
title: TUI Document Creation
type: rfc
status: accepted
author: jkaloger
date: 2026-03-05
tags:
- tui
- creation
- modal
related:
- related-to: docs/rfcs/RFC-001-my-first-rfc.md
---


## Summary

Add document creation to the TUI via a modal form overlay. Users press `n` to open a form, fill in title/author/tags/relations, and create documents without leaving the terminal UI.

Currently creation is CLI-only (`lazyspec create <type> <title>`). This makes the TUI a read-only dashboard, breaking the flow when users want to create a new document while browsing existing ones.

## Design Intent

The creation form should feel native to the existing TUI. It follows the same modal overlay pattern used by search (`/`) and help (`?`), keeping the interaction model consistent.

The form reuses `cli::create::run()` for the actual file write. The file watcher already picks up new files automatically, so no manual store update is needed. This keeps the TUI layer thin -- it only handles input collection and delegates creation to the engine.

### Form Layout

```
┌─ Create RFC ─────────────────────────────────────┐
│                                                   │
│  Title:    [____________________________________] │
│  Author:   [jkaloger___________________________ ] │
│  Tags:     [____________________________________] │
│  Related:  [____________________________________] │
│                                                   │
│          [Tab: next]  [Enter: create]  [Esc: cancel]  │
└───────────────────────────────────────────────────┘
```

### Interaction

- `n` from normal mode opens the form
- Doc type is pre-selected from the currently active type in the type panel (RFC, ADR, Story, Iteration)
- Title field is focused first
- `Tab` moves between fields
- `Shift+Tab` moves back
- `Enter` submits (creates the document)
- `Esc` cancels and returns to normal mode
- Author defaults to the value from the most recently created document or git config

### Fields

| Field | Required | Behaviour |
|-------|----------|-----------|
| Title | Yes | Free text input. Used for both frontmatter title and filename slug. |
| Author | No | Pre-filled with default. Editable. |
| Tags | No | Comma-separated. Parsed into a list on submission. |
| Related | No | Accepts shorthand like `RFC-001` or full path. Relation type prefix: `implements:RFC-001`, `related-to:ADR-002`. Defaults to `related-to` if no prefix given. |

### State Model

A new `CreateForm` struct holds the form state:

```
@draft CreateForm {
    active: bool,
    doc_type: DocType,       // from currently selected type
    focused_field: FormField, // Title | Author | Tags | Related
    title: String,
    author: String,
    tags: String,
    related: String,
    error: Option<String>,   // validation feedback
}
```

`@ref src/tui/app.rs#App` gains a `create_form: CreateForm` field. The event loop checks `app.create_form.active` as a mode, similar to `search_mode` and `fullscreen_doc`.

### Validation

Before writing:
- Title must be non-empty
- Related shorthand (e.g. `RFC-001`) is resolved via `store.resolve_shorthand()`. If unresolved, show an error in the form rather than failing silently.
- Relation type prefix must be valid (`implements`, `supersedes`, `blocks`, `related-to`)

### File Creation

On submit:
1. Call `cli::create::run(root, config, doc_type, title, author)` to write the file
2. If tags were provided, call `cli::update::run()` to set the tags field (or render them into the template directly)
3. If relations were provided, call `cli::link::link()` for each relation
4. The file watcher detects the new file and reloads the store
5. Navigate to the newly created document in the list

### Config Dependency

The TUI currently receives `&Config` as a parameter. The create form needs this to resolve directories and naming patterns. The `run()` function in `tui/mod.rs` already has access to `config`, so this is passed through when the form submits.

Additionally, the `root` path (project root / cwd) is needed for `cli::create::run()`. This is available via `app.store.root()`.

## Stories

1. **Create form UI and input handling** -- Modal overlay with title/author/tags fields, field navigation, text editing, and rendering. Wired to `n` keybinding.
2. **Document creation on submit** -- Connect form submission to `cli::create::run()`, handle tags/relations, navigate to the new document. Includes validation and error display.
