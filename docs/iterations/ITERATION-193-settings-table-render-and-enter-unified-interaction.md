---
title: Settings table render and Enter-unified interaction
type: iteration
status: accepted
author: agent
date: 2026-06-20
tags: []
related:
- implements: STORY-144
---

## Changes

Implements STORY-144 (RFC-023 Story 8). Two shifts: settings right panel `Paragraph` -> `Table`; field interaction unified on `Enter`, `Space` dropped, enum gets a variant-picker overlay.

Current anchors:
- `draw_settings` right panel = `Paragraph::new(text)` from `settings_lines`: `src/tui/views/panels.rs:2280` (table site to copy: `draw_doc_list` `src/tui/views/panels.rs:757`, Table build `:786`).
- Settings key handler: `handle_settings_key` `src/tui/views/keys.rs:734`. Space arm `:887`. Enter arm `:908` (gated by `settings_focused_is_text_entry` `:952`). Zone-editor key intercept `:794`.
- State ops: `settings_space` `src/tui/state/app.rs:1227`; `settings_cycle_enum` `:1244`; `scaffold_for_cycled_value` `:1308`; `settings_start_edit` `:1095`; `settings_open_zone_editor` `:1123`.
- Types: `FieldEditor` / `EditableField` / `FieldPath` `src/tui/state/forms.rs:356/609/559`. Overlay models: `StatusPicker` `:218`, `ZoneOrderingEditor` `:390`. Overlay render `draw_settings_zone_editor` `src/tui/views/overlays.rs:376`, `draw_status_picker` `:452`. Overlay dispatch `src/tui/views.rs:205-236`.

### Task 1 -- VariantPicker overlay state + render

ACs: AC4 (enum opens picker).

- `src/tui/state/forms.rs`: add `pub struct SettingsVariantPicker { pub path: FieldPath, pub variants: &'static [&'static str], pub selected: usize }` + impl `new(path, variants, current_index)`, `cursor_up`, `cursor_down` (clamp to `variants.len()-1`, like `ZoneOrderingEditor::cursor_up/down` `:451`). Pure, no terminal.
- `src/tui/state/app.rs` App struct: add field `pub settings_variant_picker: Option<SettingsVariantPicker>` (default `None`); init alongside other settings_* fields (find where `settings_zone_editor` is declared/initialised, mirror it).
- `src/tui/views/overlays.rs`: add `pub fn draw_settings_variant_picker(f, app)`, model on `draw_status_picker` `:452` (centered popup, `Clear`, `List` of variants, `ListState` selected = picker.selected, title ` Select value `, hint line `[j/k: move] [Enter: select] [Esc: cancel]`).
- `src/tui/views.rs`: in overlay block (`:218-236`), add `if app.settings_variant_picker.is_some() { draw_settings_variant_picker(f, app); }`; add to `use` import list (`:29-31`).

Verify: `cargo build`. Picker not yet reachable (keys land Task 3).

### Task 2 -- Extract enum-variant write (scaffold parity)

ACs: AC5 (scaffold-on-pick parity).

- `src/tui/state/app.rs`: refactor `settings_cycle_enum` `:1244`. Extract the per-`FieldPath` write + `scaffold_for_cycled_value` call (body `:1261-1298`) into `fn settings_set_enum_variant(&mut self, path: &FieldPath, variant: &str)`. Keep the rule-`shape` special case (`settings_cycle_rule_shape` `:1255`) reachable from the new fn when `variant` names the other shape; picker selecting a shape variant must convert the whole rule like the cycle path did.
- `settings_cycle_enum` keeps computing `next` from current+1 then calls `settings_set_enum_variant(path, next)` -- behaviour identical for any remaining caller (none after Task 3, but keep the fn for the unit tests / no orphan removal yet).

Verify: `cargo test settings` green (existing scaffold tests still pass via cycle path).

### Task 3 -- Enter dispatch, drop Space, picker keys

ACs: AC2 (text-family inline edit), AC3 (bool flip), AC4 (enum picker open), AC6 (zone editor), AC7 (readonly no-op), AC8 (Space no-op), AC9 (entry-list drill-in unchanged).

`src/tui/views/keys.rs` `handle_settings_key`:
- Add picker intercept before nav, modelled on zone-editor intercept `:794`: when `settings_variant_picker.is_some()` -> `j`/`Down` cursor_down, `k`/`Up` cursor_up, `Enter` commit (read `variants[selected]`, call `self.settings_set_enum_variant(&path, variant)`, set `settings_dirty`, take/clear picker), `Esc` clear picker. `return` after.
- Delete the `Space` arm `:887-889`. (Leave dashboard/tree Space untouched -- different handler.)
- Rewrite the `Enter` arm `:908-924`: if `settings_in_entry_list()` -> drill in (unchanged, AC9). Else dispatch on `self.settings_focused_field().editor`:
  - `Text|BoundedNum|Nullable|Duration|List` -> `settings_start_edit()` (AC2)
  - `Toggle` -> flip in buffer + dirty (move logic from `settings_space` Toggle branch `app.rs:1232`; add `fn settings_toggle_bool(&mut self)` or inline) (AC3)
  - `EnumCycle { variants }` -> open picker: compute current index from `settings_focused_raw()`, set `settings_variant_picker = Some(SettingsVariantPicker::new(path, variants, idx))` (AC4)
  - `ZoneOrdering` -> `settings_start_edit()` already routes to zone editor (AC6) -- keep via start_edit
  - `ReadOnly` -> no-op (AC7)
- Remove now-unused `settings_focused_is_text_entry` `:952` (gate gone) or repurpose; remove `settings_space` `app.rs:1227` only if no caller remains (Task 2 may keep it referenced by tests -- if so leave it, else delete).
- AC8: with Space arm deleted, `Space` falls to `_ => {}` -> no-op.

Verify: `cargo build`; `cargo run` -> `5` settings -> Enter on a bool flips, Enter on `numbering` opens picker, Enter on text edits, Space does nothing.

### Task 4 -- Right panel as Table + migrate edit/error/scaffold render

ACs: AC1 (table render), AC10 (error/prompt survive).

`src/tui/views/panels.rs` `draw_settings` `:2280`, right-panel section `:2344-end`:
- Replace `Paragraph` build with `Table::new(rows, widths)` (model `draw_doc_list` `:786`). Rows from `settings_fields(...)` (`:2347`): field-view -> two cols `[label, value]`; header `Row::new(["Field","Value"])`. Widths e.g. `[Constraint::Percentage(40), Constraint::Percentage(60)]`. `TableState` selected = `app.settings_field.min(field_count-1)`; `row_highlight_style(REVERSED)` like doc list -- replaces the manual cyan-restyle of the focused line.
- Entry-list view (`settings_in_entry_list`): render as single-col Table (entry name rows) with same selection highlight; drop the `▸` inline marker (selection cursor replaces it).
- Editing focused row: when `app.settings_editing`, render that row's value cell as live `settings_display_value(f, true, &app.settings_edit_input)` (current logic `:2371-2378`) so caret shows in-cell.
- Edit-error (`:2403-2414`): move from `text.insert` into a one-line red footer under the table (reuse/extend the footer-split layout already used for `settings_footer_error` `:2329-2342`; show field error there while editing).
- Scaffold offer prompt (`:2419-2432`) + sqids salt `(required)` marker (`:2359-2367`, `:2388-2394`): render salt-required as a styled value cell (yellow); render scaffold "press g" prompt in the footer line.
- `settings_lines` / `settings_lines_inner` stay as the pure value source for existing tests; Table cells derive from `settings_fields` (already the source). Do not delete those fns.

Verify: `cargo build`; `cargo run` -> settings panel shows bordered table with header + selection bar matching Documents list; edit error shows in footer; scaffold prompt + salt-required still visible.

### Task 5 -- Tests

See Test Plan. Add state/keys unit tests; reuse existing `settings_fields`/`settings_lines_inner` tests for field content.

## Test Plan

Pure (App/state, no terminal) -- isolated, deterministic, behavioral:
- `SettingsVariantPicker`: new sets selected=current index; cursor_down/up clamp at bounds. (AC4)
- Enter dispatch (drive `handle_settings_key` on a built `App`):
  - bool field + Enter -> buffer value flipped, `settings_dirty` true. (AC3)
  - enum field (`numbering`) + Enter -> `settings_variant_picker.is_some()`, variants == numbering set, selected == current. (AC4)
  - text field + Enter -> `settings_editing` true. (AC2)
  - readonly field + Enter -> no state change. (AC7)
  - any field + Space -> no state change (no flip, no picker, no edit). (AC8)
  - entry-list (cat 1, drill None) + Enter -> `settings_drill == Some(entry)`. (AC9)
- Picker commit: open picker on a type `numbering`, move to `sqids`, Enter -> buffer numbering == sqids AND `[numbering.sqids]` scaffolded (assert via buffer + scaffold offer), matching pre-existing cycle-path scaffold test. (AC5)
- Picker Esc -> picker None, buffer unchanged. (AC4)

Field content (existing, keep): `settings_fields` / `settings_lines_inner` tests `panels.rs:3028+` -- table cells derive from `settings_fields`, so these still cover label/value correctness. (AC1 content)

Render: the `Table` widget draw itself is not asserted (consistent with repo -- `draw_doc_list` has no render-output test). Visual correctness (header, selection bar, footer error placement, scaffold prompt) verified manually via `cargo run`. (AC1 layout, AC10)

Tradeoff: full TUI render-snapshot testing (e.g. ratatui `TestBackend` buffer assertions) would cover AC1/AC10 layout deterministically but no settings render is tested that way today; staying with pure-state tests + manual visual check matches the existing test surface and avoids introducing a backend-snapshot harness in this iteration.

## Notes

- `Space` is dropped only in settings field-view. Dashboard tree-expand Space (`keys.rs handle_normal_key` `:1037`) and zone-editor Space (`keys.rs:811`, its own modal) are untouched.
- Zone editor reached via `Enter` already (`settings_start_edit` routes `ZoneOrdering` to `settings_open_zone_editor` `app.rs:1100`); no change needed for AC6 beyond keeping Enter->start_edit for that editor kind.
- Help overlay (`overlays.rs:15`) has no settings-field Space hint to remove; its `Space` line is dashboard tree-expand. No help edit required; STORY-144 "update help/keybinding hints" is satisfied by confirming none reference settings Space.
- RFC-023 already amended this session (interaction model, editing table, Story 8) -- no further doc change.
