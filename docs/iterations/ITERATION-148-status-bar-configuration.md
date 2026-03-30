---
title: Status bar configuration
type: iteration
status: accepted
author: agent
date: 2026-03-30
tags: []
related:
- implements: STORY-107
---



## Changes

### Task 1: Add `StatusBarConfig` to the engine config layer

ACs addressed: AC-1 (defaults when section absent), AC-2 (`enabled = false`), AC-3 (`enabled = true` or omitted), AC-4 (custom zone arrays), AC-5 (partial zone overrides keep defaults), AC-7 (empty arrays)

Files:
- Modify: `src/engine/config.rs`

Add a `StatusBarConfig` struct with `enabled: bool` (default `true`) and optional `left`, `center`, `right` fields (`Option<Vec<String>>`). Nest it inside `UiConfig` as `pub statusbar: StatusBarConfig` so it deserializes from `[tui.statusbar]`.

`StatusBarConfig` provides a `Default` impl that sets `enabled = true` and all zone fields to `None`. When a zone field is `None`, the consumer uses the hardcoded defaults from `StatusBarComponents::default()`. When a zone field is `Some(vec)`, that list is used verbatim (including `Some(vec![])` for intentionally empty zones).

`UiConfig` gets `#[serde(default)]` on the `statusbar` field so that omitting `[tui.statusbar]` entirely produces the defaults.

### Task 2: Build `StatusBarComponents` from config with validation warnings

ACs addressed: AC-4 (custom zone arrays), AC-5 (partial overrides), AC-6 (invalid component names ignored with warning), AC-7 (empty arrays)

Files:
- Modify: `src/tui/views/status_bar.rs`

Add a `const` or static lookup that maps component name strings (`"mode"`, `"type_filter"`, `"doc_count"`, `"warnings"`, `"errors"`, `"version"`, `"help_hint"`, `"search"`, `"git_branch"`) to their corresponding `StatusComponent` functions.

Add `StatusBarComponents::from_config(config: &StatusBarConfig) -> (Self, Vec<String>)`. This method resolves each zone: if the config zone is `None`, use the default component list; if `Some`, map each string to its component function via the lookup. Unknown names are skipped and collected into the returned `Vec<String>` of warnings. Empty `Some(vec![])` produces an empty zone vec.

### Task 3: Wire config through draw call sites

ACs addressed: AC-2 (`enabled = false` hides bar and reclaims row), AC-3 (visible when enabled)

Files:
- Modify: `src/tui/views.rs`
- Modify: `src/tui/state/app.rs`

Store the resolved `StatusBarComponents` and `enabled` flag on `App` during construction (in `App::new`), built from `config.ui.statusbar`. Log any validation warnings from Task 2 at this point.

In `src/tui/views.rs`, replace the three `StatusBarComponents::default()` call sites. When `enabled` is false, skip the `draw_status_bar` call and reclaim the row (don't reserve bottom row in the layout split). When enabled, pass the stored components.

## Test Plan

Tests go in `tests/tui_status_bar_test.rs` (extends the existing file).

1. `default_config_produces_default_components` -- Construct a `StatusBarConfig::default()`, call `from_config`, assert the resulting component counts match `StatusBarComponents::default()` (left=3, center=2, right=4) and no warnings are emitted. Verifies AC-1.

2. `custom_left_zone_overrides_default` -- Construct `StatusBarConfig` with `left: Some(vec!["mode".into()])`, `center: None`, `right: None`. Call `from_config`. Assert left has 1 component, center has 2, right has 4. Verifies AC-4 and AC-5.

3. `empty_zone_array_produces_empty_zone` -- Construct with `left: Some(vec![])`. Assert left is empty after `from_config`. Verifies AC-7.

4. `invalid_component_name_skipped_with_warning` -- Construct with `left: Some(vec!["mode".into(), "bogus".into()])`. Assert left has 1 component and warnings contains `"bogus"`. Verifies AC-6.

5. `config_round_trip_through_toml` -- Parse a TOML string with `[tui.statusbar]` section into `Config`, assert `StatusBarConfig` fields match. Then parse a TOML string _without_ the section, assert defaults. Verifies AC-1 and AC-3 at the serde layer.

6. `enabled_false_in_toml` -- Parse TOML with `enabled = false`, assert the field is `false`. Verifies AC-2 at the serde layer. (The rendering effect of `enabled = false` is tested through the existing TUI draw tests if they exist, or manually verified.)

## Notes

The validation warnings for invalid component names (AC-6) are surfaced during `App::new`. The exact mechanism (log, stderr, or stored on App for display) should match how the codebase handles other config warnings. Check existing patterns before choosing.
