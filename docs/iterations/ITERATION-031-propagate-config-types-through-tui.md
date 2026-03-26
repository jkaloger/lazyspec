---
title: Propagate config types through TUI
type: iteration
status: accepted
author: agent
date: 2026-03-07
tags: []
related:
- implements: STORY-039
---




## Changes

### Task 1: Pass Config into App and populate doc_types from config

**ACs addressed:** AC-6 (TUI type tab bar shows configured types), AC-8 (default config identical)

**Files:**
- Modify: `src/tui/app.rs`
- Modify: `src/tui/mod.rs`

**What to implement:**

Change `App::new` to accept `&Config` as a second parameter. Replace the hardcoded `doc_types` vec (lines 232-237) with:

```rust
doc_types: config.types.iter().map(|t| DocType::new(&t.name)).collect(),
```

Store an icon lookup map on `App` for use by the graph renderer:

```rust
pub type_icons: HashMap<String, String>,
```

Populate it in `App::new`:

```rust
let default_glyphs = ["●", "■", "▲", "◆", "★", "◎"];
let type_icons: HashMap<String, String> = config.types.iter().enumerate().map(|(i, t)| {
    let icon = t.icon.clone().unwrap_or_else(|| default_glyphs[i % default_glyphs.len()].to_string());
    (t.name.clone(), icon)
}).collect();
```

In `src/tui/mod.rs` line 49, pass config to `App::new`:

```rust
let mut app = App::new(store, config);
```

**How to verify:**
```
cargo test tui_
```

### Task 2: Use type_icons in graph rendering

**ACs addressed:** AC-7 (graph icon from config with fallback glyphs)

**Files:**
- Modify: `src/tui/ui.rs`

**What to implement:**

Replace the hardcoded icon match at lines 905-909:

```rust
let type_icon = if node.doc_type == DocType::new(DocType::RFC) { "●" }
    else if node.doc_type == DocType::new(DocType::ADR) { "■" }
    else if node.doc_type == DocType::new(DocType::STORY) { "▲" }
    else if node.doc_type == DocType::new(DocType::ITERATION) { "◆" }
    else { "○" };
```

With a lookup against `app.type_icons`:

```rust
let type_icon = app.type_icons
    .get(&node.doc_type.to_string())
    .map(|s| s.as_str())
    .unwrap_or("○");
```

**How to verify:**
```
cargo test tui_
```

### Task 3: Update test call sites for App::new signature

**ACs addressed:** AC-8 (default config identical -- tests prove no behavioral change)

**Files:**
- Modify: `tests/tui_create_form_test.rs`
- Modify: `tests/tui_delete_dialog_test.rs`
- Modify: `tests/tui_editor_test.rs`
- Modify: `tests/tui_filters_test.rs`
- Modify: `tests/tui_fullscreen_test.rs`
- Modify: `tests/tui_graph_test.rs`
- Modify: `tests/tui_handle_key_test.rs`
- Modify: `tests/tui_navigation_test.rs`
- Modify: `tests/tui_relations_test.rs`
- Modify: `tests/tui_search_test.rs`
- Modify: `tests/tui_submit_form_test.rs`
- Modify: `tests/tui_view_mode_test.rs`

**What to implement:**

Every `App::new(store)` call becomes `App::new(store, &fixture.config())`. All tests already have a `TestFixture` with a `config()` method returning `Config::default()`.

This is a mechanical find-and-replace. The tests verify that default config produces identical behavior (AC-8).

**How to verify:**
```
cargo test tui_
```

### Task 4: Add test for custom types in TUI

**ACs addressed:** AC-6 (type tab bar shows custom types), AC-7 (graph icon for custom type)

**Files:**
- Modify: `tests/tui_graph_test.rs`

**What to implement:**

Add a test that creates a `Config` with custom types (e.g. `epic`, `task`) and verifies:

1. `app.doc_types` contains the custom types (not the default four)
2. `app.type_icons` maps each custom type to its configured icon, or a fallback glyph when no icon is set

```rust
#[test]
fn custom_types_populate_doc_types_and_icons() {
    let fixture = TestFixture::new();
    let mut config = fixture.config();
    config.types = vec![
        TypeDef { name: "epic".into(), plural: "epics".into(), dir: "docs/epics".into(), prefix: "EPIC".into(), icon: Some("⚡".into()) },
        TypeDef { name: "task".into(), plural: "tasks".into(), dir: "docs/tasks".into(), prefix: "TASK".into(), icon: None },
    ];
    let store = Store::load(fixture.root(), &config).unwrap();
    let app = App::new(store, &config);

    assert_eq!(app.doc_types.len(), 2);
    assert_eq!(app.doc_types[0], DocType::new("epic"));
    assert_eq!(app.doc_types[1], DocType::new("task"));
    assert_eq!(app.type_icons["epic"], "⚡");
    assert_eq!(app.type_icons["task"], "●"); // first fallback glyph
}
```

**How to verify:**
```
cargo test custom_types_populate
```

## Test Plan

**Default behavior unchanged (Task 3):**
All ~20 existing TUI tests pass after the signature change. This is the primary regression gate -- if `cargo test tui_` passes with `Config::default()`, AC-8 is satisfied. These tests are fast, isolated, and deterministic.

**Custom types in doc_types (Task 4):**
A new unit test creates a Config with non-default types and asserts `app.doc_types` reflects them. Verifies AC-6 at the data level. Behavioral (tests the mapping, not the struct shape). Specific (failure tells you exactly which type is wrong).

**Custom type icons (Task 4):**
Same test asserts `app.type_icons` uses configured icons and falls back to default glyphs. Verifies AC-7 at the data level. The actual rendering in `draw_graph` is covered by the lookup change being trivially correct against the map.

> [!NOTE]
> Graph rendering is not tested at the pixel level (ratatui Frame tests are heavyweight and brittle). The icon lookup is simple enough that verifying the map contents is sufficient.

## Notes

The CLI side (create, init, store, template) was already propagated in ITERATION-028 Task 3. This iteration covers only the remaining TUI hardcoded spots.

The `Directories` struct still exists on `Config` alongside `types: Vec<TypeDef>`. Removing it is out of scope for this iteration -- it's a separate cleanup concern.
