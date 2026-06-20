---
title: Settings Interface category and status bar ordering
type: iteration
status: accepted
author: agent
date: 2026-06-19
tags: []
related:
- implements: STORY-143
---

## Changes

Slice 7 of RFC-023. Implements STORY-143 = Interface settings category. Builds ON slice 1 (ITER-186: `ViewMode::Settings`, category list incl Interface, j/k field nav, read-only right-panel render) + slice 3 (ITER-188: `FieldEditor` enum, `EditableField{label,path,editor}`, `FieldPath`, `SettingsState{buffer:Config,dirty,editing,edit_input,edit_error,footer_error}`, `settings_start_edit/confirm_edit/space`, `settings_save`, `write_config_in_place`). This slice = attach Interface-category editable fields to those seams + add zone-ordering editor for `statusbar.left/center/right`.

Config surface (VERIFIED): `UiConfig` config.rs:250-258 `{ascii_diagrams:bool, statusbar:StatusBarConfig, multiline:MultiLineConfig}`; `Config.ui` `#[serde(rename="tui")]` config.rs:270. `StatusBarConfig` config.rs:205-215 `{enabled:bool(default true config.rs:217), left/center/right:Option<Vec<String>>}` derives `PartialEq`. `MultiLineConfig` config.rs:236-240 `{max_expanded_height:usize}` default `5` (config.rs:232-234).

RFC-022 status-bar component vocabulary (VERIFIED, already implemented — see Notes): the authoritative set = the names matched by `lookup_component` `src/tui/views/status_bar.rs:143-156`: `["mode","type_filter","doc_count","warnings","errors","version","help_hint","search","git_branch"]` (9 names). `StatusBarComponents::from_config(&StatusBarConfig)->(Self,Vec<String>)` status_bar.rs:159-184 already resolves a `Vec<String>` of names → `Vec<StatusComponent>`, warning on unknown via `resolve_names` status_bar.rs:187-197. Defaults `StatusBarComponents::default()` status_bar.rs:199-212 (left mode/type_filter/doc_count, center warnings/errors, right git_branch/search/version/help_hint).

### Task 1 — Expose RFC-022 component vocabulary as a single shared `const`
ACs: AC5.
Files: `src/tui/views/status_bar.rs`.
- Add `pub const STATUS_BAR_COMPONENTS: &[&str] = &["mode","type_filter","doc_count","warnings","errors","version","help_hint","search","git_branch"];` — the exact name set `lookup_component` (status_bar.rs:143-156) accepts.
- Refactor `lookup_component` to remain the single source of truth: the const MUST list exactly the names `lookup_component` returns `Some(..)` for. Add a unit test (Task in Test Plan AC5) asserting every name in `STATUS_BAR_COMPONENTS` resolves via `lookup_component(name).is_some()` AND that the count matches (no name in the const is unknown, guards drift).
- Rationale: ordering editor (Task 3/4) offers selectable names FROM this const, so the offered set == RFC-022 vocabulary by construction; a name not in the const (hence not in `lookup_component`) is never offered (AC5).
Verify: `cargo build`; the AC5 test below.

### Task 2 — Interface category editable field list (boolean + bounded-numeric rows)
ACs: AC1 AC2 AC3.
Files: wherever slice-1 builds the per-category `Vec<EditableField>` (settings field model — `src/tui/state/app.rs` SettingsState field-list builder, or `src/tui/state/forms.rs` if slice 1 put it there); reuse slice-3 `FieldEditor` (forms.rs) + `FieldPath`.
- Build the Interface category field list (the rows AC1 requires), in this order, each an `EditableField{label,path,editor}` whose `FieldPath` targets the `[tui]` buffer subtree Task-1-of-slice-3's `write_config_in_place` already mutates (`ascii_diagrams`, `statusbar.enabled`, `multiline.max_expanded_height` — confirmed handled config_write.rs per ITER-188 Task 1):
  1. `ascii_diagrams` → `FieldEditor::Toggle`, path `tui.ascii_diagrams`. (AC1, AC2)
  2. `multiline.max_expanded_height` → `FieldEditor::BoundedNum{min,max}`, path `tui.multiline.max_expanded_height`. Choose a sane positive bound: `min:1` (0-height expansion is meaningless), `max` a generous cap (e.g. `100`). Unset/absent section → row shows default `5` (read from `MultiLineConfig::default` config.rs:242-247 via `UiConfig`'s `#[serde(default)]` config.rs:256 — buffer always has a concrete `usize`, no `(unset)` for this field). (AC1, AC3)
  3. `statusbar.enabled` → `FieldEditor::Toggle`, path `tui.statusbar.enabled`. Unset section → buffer carries default `true` (config.rs:217), row reflects it. (AC1, AC2)
  4. `statusbar.left` → zone-ordering editor (Task 3), path `tui.statusbar.left`. (AC1, AC4)
  5. `statusbar.center` → zone-ordering editor, path `tui.statusbar.center`. (AC1, AC4)
  6. `statusbar.right` → zone-ordering editor, path `tui.statusbar.right`. (AC1, AC4)
- Row render value (AC1, read-only display before edit): bool → `true`/`false`; numeric → the `usize`; each zone → comma-joined current `Vec<String>` or, when the zone's `Option` is `None`, render the RFC-022 default for that zone (`StatusBarComponents::default()` zone names status_bar.rs:199-212) marked as default (e.g. `mode, type_filter, doc_count (default)`), since `None` means "use defaults" per `from_config` status_bar.rs:163-174 — do NOT render `(unset)` blank, the bar is non-empty by default.
- `ascii_diagrams` + `statusbar.enabled` edit via the EXISTING slice-3 `Toggle` path (`settings_space` flips buffer bool at `FieldPath`, `dirty=true` — ITER-188 Task 4): no new edit code, just register the fields. (AC2)
- `max_expanded_height` edits via the EXISTING slice-3 `BoundedNum` path (`settings_confirm_edit` → `validate_bounded(input,min,max)` rejects out-of-range/non-numeric, keeps prior value + sets `edit_error`, accepts valid → writes `usize`, `dirty=true` — ITER-188 Task 5). No new validation code. (AC3)
Verify: Test Plan AC1/AC2/AC3 App-state units.

### Task 3 — Zone-ordering editor model (the distinctive control)
ACs: AC4 AC5.
Files: `src/tui/state/forms.rs` (new ordering-editor state); `src/tui/state/app.rs` (settings state extension).
- Add `FieldEditor::ZoneOrdering` variant (forms.rs, alongside slice-3 Text/Toggle/BoundedNum/Nullable/Duration/List/EnumCycle). It edits a `Option<Vec<String>>` zone whose member names are constrained to `STATUS_BAR_COMPONENTS` (Task 1). (AC5)
- Add `pub struct ZoneOrderingEditor` (forms.rs): `{ selected: Vec<String>, // ordered, in-zone components; available: Vec<&'static str>, // STATUS_BAR_COMPONENTS minus those in selected; cursor: usize, // focus within the editor; pane: ZonePane }` where `enum ZonePane { Selected, Available }`. Init from the focused zone buffer value: `Some(names)` → `selected = names.clone()` (preserving order); `None` → `selected = default zone names` (the RFC-022 default for that zone, so the editor opens reflecting what's actually shown). `available` = `STATUS_BAR_COMPONENTS` filtered to names not in `selected`. (AC4 add/remove/reorder; AC5 vocabulary = the const only.)
- Editor operations (pure on `ZoneOrderingEditor`, App-testable):
  - add: move name from `available` → end of `selected` (AC4 add). Source = const → AC5 guaranteed.
  - remove: move name from `selected` → `available` (AC4 remove).
  - move-up / move-down: swap `selected[cursor]` with neighbour (AC4 reorder), in-zone reorder.
- Extend settings state (slice-1 `SettingsState` on `App`) with `pub zone_editor: Option<ZoneOrderingEditor>` (Some while editing a zone, mirrors slice-3 `editing`/`edit_input` scratch). `settings_start_edit` for a `ZoneOrdering` field constructs `ZoneOrderingEditor` from the buffer zone instead of seeding `edit_input`.
Verify: Test Plan AC4/AC5 — construct editor from a zone, exercise add/remove/move ops, assert `selected` order + that `available ∪ selected ⊆ STATUS_BAR_COMPONENTS`.

### Task 4 — Zone-ordering key dispatch + commit into buffer (slice-3 dirty/save reuse)
ACs: AC4.
Files: `src/tui/views/keys.rs` (extend slice-3 `handle_settings_key`); `src/tui/state/app.rs`.
- In `handle_settings_key` (slice-3 ITER-188 Task 3, keys.rs), when `self.settings.zone_editor.is_some()` route keys to the ordering editor instead of the scalar edit path:
  - `Tab` → toggle `pane` (Selected ↔ Available).
  - `j`/`k` → move `cursor` within the active pane.
  - `Space`/`Enter` on Available pane → add focused name to `selected`. On Selected pane → remove focused name (move back to Available). (AC4 add/remove)
  - `K`/`J` (shift-up/down) on Selected pane → move-up/move-down focused name. (AC4 reorder)
  - `Enter`-to-commit affordance: bind commit to a non-conflicting key — `w` or `Ctrl-S` are global save (slice-3), so use `Esc`-confirms-into-buffer matching RFC-023 "Enter again confirms the field into the buffer" semantics: on a dedicated commit key (`c` or a second `Enter` when nothing focused), call `settings_commit_zone()`; plain `Esc` cancels editor (drop `zone_editor`, buffer untouched). Pick one and document; commit writes, cancel discards.
- `settings_commit_zone(&mut self)`: write the editor's `selected` into the buffer zone at `FieldPath` per slice-3 list semantics (AC4): non-empty `selected` → `Some(selected.clone())` (chosen order); empty `selected` (user removed all) → explicitly-cleared zone persists as `Some(vec![])` / absent per slice-3 `List` semantics (ITER-188 Task 3 `List`: empty → `vec![]`; here `Option<Vec<String>>` zone, empty cleared zone → `Some(vec![])` so it's an explicit empty, distinct from untouched `None`). A zone the user never opened stays `None` (untouched, saved as before — AC4). Set `dirty=true`, `zone_editor=None`.
- Persist via EXISTING slice-3 atomic save: `w`/`Ctrl-S` → `settings_save` (ITER-188 Task 6) renders buffer via `write_config_in_place` (which already handles `[tui.statusbar]` keys per ITER-188 Task 1 + must handle the three zone arrays — CONFIRM `write_config_in_place` writes `statusbar.left/center/right` `Option<Vec<String>>` as toml arrays / removes key when `None`; if slice-3's writer only did `enabled`, extend it here to emit the three zone arrays). One atomic write. (AC4 persisted)
Verify: Test Plan AC4 — commit ordered zone → buffer `Some(vec![..])` in order; untouched zone stays `None`; cleared zone → empty list.

### Task 5 — AC6 BUILD-gate reconciliation (no code)
ACs: AC6.
Files: NONE (documentation only — see Notes).
- AC6 = scheduling constraint ("don't BUILD until RFC-022's vocabulary + rendering land"). INVESTIGATION RESULT: RFC-022 is ALREADY implemented (status_bar.rs vocabulary + `from_config` + `draw_status_bar` wired in views.rs, `[tui.statusbar]` parsed in config.rs). The gate is therefore SATISFIED — no waiting, no stub. Recorded in Notes; no test (gate is met, not testable code).

## Test Plan

One entry per AC. App-state / pure-model units on the editor + commit methods (no live terminal); reuse slice-3 seams (`settings_*` methods, `SettingsState`, `FieldEditor`). AC6 is a scheduling gate (N/A as a test) — documented, not coded.

- **AC1 (Interface surfaces full UiConfig)**: App-state unit. Build the Interface category field list (Task 2) from a `SettingsState` seeded with a default `Config`. Assert the field list contains exactly 6 rows with labels/paths `tui.ascii_diagrams`, `tui.multiline.max_expanded_height`, `tui.statusbar.enabled`, `tui.statusbar.left`, `tui.statusbar.center`, `tui.statusbar.right`, in that order; assert each row's read value reflects defaults (`ascii_diagrams=false`, `max_expanded_height=5`, `statusbar.enabled=true`, zones render their RFC-022 default names since `None`). Then seed a `Config` with explicit `[tui]` values and assert rows reflect them. Seam: Interface field-list builder. Property: asserts the rows + their editor variants exist and reflect buffer/defaults, not internal layout.
- **AC2 (boolean inline edit + persist)**: App-state unit. Focus `tui.ascii_diagrams` (Toggle), capture prior bool, `settings_space()` → assert buffer `ui.ascii_diagrams` flipped, `dirty==true`; repeat → flips back. Repeat for `tui.statusbar.enabled` → assert `ui.statusbar.enabled` flips. Persist leg: temp-file `settings_save` (slice-3 path) after a toggle → re-`Config::parse` of written `.lazyspec.toml` shows the new boolean in `[tui]`. Seam: `settings_space` + `settings_save` (no new code, just the registered fields). Property: asserts the UiConfig value + the persisted `[tui]` boolean.
- **AC3 (bounded numeric accept/reject/default)**: App-state unit + pure. Pure: reuse slice-3 `validate_bounded(input,min,max)` — assert `"0"`→Err (below min 1), `"-1"`/`"abc"`→Err, `"5"`→Ok(5), `"100"`→Ok (at max). App-state: focus `tui.multiline.max_expanded_height`, edit `"0"`, confirm → buffer keeps prior `5`, `edit_error.is_some()`, `dirty` unchanged, `editing` still true; edit `"8"`, confirm → buffer `max_expanded_height==8`, `dirty==true`. Unset/default: a `Config` with no `[tui.multiline]` → row shows `5`. Seam: `validate_bounded` (pure) + `settings_confirm_edit`. Property: boundary (0 reject / 1 & max accept) + prior-value retention on reject.
- **AC4 (zone ordering round-trip + clear vs untouched)**: App-state / model unit. Construct `ZoneOrderingEditor` for `tui.statusbar.left` from buffer (`None` → seeded with default left zone names). Exercise: remove `doc_count`, add `git_branch`, move `git_branch` up one → `settings_commit_zone()` → assert buffer `ui.statusbar.left == Some(vec![...])` in the exact resulting order, `dirty==true`. Untouched leg: never open `center`/`right` → after committing `left` + `settings_save` (temp file), re-parse shows `center`/`right` saved AS BEFORE (`None` → absent / unchanged). Cleared leg: open a zone, remove all → commit → buffer zone `Some(vec![])` (explicitly cleared, distinct from untouched `None`); after save re-parse, the zone persists as an explicit empty/absent list per slice-3 list semantics. Seam: `ZoneOrderingEditor` ops + `settings_commit_zone` + `settings_save`. Property: order preserved + the None(untouched) vs Some(vec![])(cleared) distinction is load-bearing.
- **AC5 (ordering editor offers exactly RFC-022 vocabulary)**: pure unit. (a) Vocabulary-membership invariant: for every `name` in `STATUS_BAR_COMPONENTS` (Task 1), assert `lookup_component(name).is_some()` (every offered name resolves to a real component); and assert NO extra resolvable name is missing from the const (drift guard — e.g. enumerate the const and assert len == number of `Some` arms, or assert a representative out-of-vocab name like `"clock"`/`"battery"` is NOT in the const AND `lookup_component("clock").is_none()`). (b) Editor `available`+`selected` are drawn only from `STATUS_BAR_COMPONENTS`: construct `ZoneOrderingEditor`, assert `available ∪ selected ⊆ STATUS_BAR_COMPONENTS` and adding can only ever surface a const name (a name not in the const can never enter `selected`). Seam: `STATUS_BAR_COMPONENTS` const + `lookup_component` (status_bar.rs) + `ZoneOrderingEditor`. Property: the offered set == RFC-022 vocabulary, by construction.
- **AC6 (BUILD gated on RFC-022)**: N/A — scheduling constraint, not behaviour. Gate documented in Notes: RFC-022 is already implemented (vocabulary `lookup_component` status_bar.rs:143-156, `from_config` status_bar.rs:159, rendering `draw_status_bar` wired views.rs:79/98/209, `[tui.statusbar]` parsed config.rs:205-215), so the gate is satisfied and BUILD may proceed. No test asserts a scheduling decision.

## Notes

- **AC6 / RFC-022 status — RECONCILIATION (investigated `src/tui/views/status_bar.rs` per instructions).** STORY-143 states "RFC-022 accepted but not yet implemented" and gates BUILD on it landing. INVESTIGATION FINDING: this is STALE — **RFC-022 is fully implemented.** Evidence:
  - Component vocabulary EXISTS: `lookup_component` status_bar.rs:143-156 resolves all 9 RFC-022 components (`mode, type_filter, doc_count, warnings, errors, version, help_hint, search, git_branch`); each has a `fn(&App)->Option<Span>` impl (status_bar.rs:227-300).
  - `StatusBarComponents::from_config(&StatusBarConfig)->(Self,Vec<String>)` EXISTS (status_bar.rs:159-184) — resolves config name lists → components, warns on unknown names (`resolve_names` status_bar.rs:187-197), falls back to defaults when a zone is `None`.
  - Rendering is WIRED LIVE: `draw_status_bar` (status_bar.rs:124-135) called from `views.rs:79,98,209`, gated by `app.status_bar_enabled` (views.rs:73/92/111/208), components built at startup `StatusBarComponents::from_config(&config.ui.statusbar)` (app.rs:354), `enabled` honoured (app.rs:453).
  - `[tui.statusbar]` config parses today: `StatusBarConfig` config.rs:205-215 with `enabled`(default true) + `left/center/right: Option<Vec<String>>`.
  - RFC-022 doc status = **`accepted`** (RFC-022-tui-status-bar.md:4), and its three stories (widget+components, git/search/type_filter, `[tui.statusbar]` config) are all evidenced as built in source.
  - CONCLUSION: the AC6 BUILD gate is **already satisfied** — the RFC-022 dependency (component vocabulary + rendering) has landed. No remaining RFC-022 work blocks this slice. The slice can proceed to BUILD. (Recommend updating STORY-143's "not yet implemented" wording at planning time; left as-is here since this is an iteration body.)
- **Genuinely-remaining dependency = slice 3 (ITER-188), NOT RFC-022.** This iteration's only hard build dep is the slice-3 editor/save machinery (`FieldEditor`, `EditableField`, `FieldPath`, `SettingsState` dirty buffer, `settings_start_edit/confirm_edit/space`, `validate_bounded`, `settings_save`, `write_config_in_place`) and slice 1's category list + field nav. Both siblings (ITER-186, ITER-188) were TODO/in-progress stubs when this was authored, so exact `SettingsState` field names + `handle_settings_key` shape follow those slices once landed — tasks here reference the seam by name.
- **Distinctive control = the zone-ordering editor** (Tasks 3/4). It is the one piece NOT covered by slice 3's scalar/list editors: a two-pane (Selected / Available) reorderable picker whose candidate set is sourced from `STATUS_BAR_COMPONENTS` (Task 1), composing RFC-022's vocabulary into each zone in user-chosen order, committing `Option<Vec<String>>` per slice-3 list semantics (untouched `None` vs explicit empty `Some(vec![])`).
- **`STATUS_BAR_COMPONENTS` const (Task 1)** is the seam that makes AC5 true by construction: the editor offers names from the const, and the const is kept == `lookup_component`'s accepted set (AC5 drift-guard test). Adding a new RFC-022 component later = add an arm to `lookup_component` + a name to the const (one place each).
- **`write_config_in_place` zone arrays:** ITER-188 Task 1 lists `statusbar.enabled` among handled `[tui.statusbar]` keys but its enumerated key list may not include the three zone arrays. Task 4 CONFIRMS/EXTENDS the slice-3 writer to emit `statusbar.left/center/right` as toml arrays (and remove the key when the zone is `None`/untouched) so committed orderings round-trip through the atomic save. No second write path — reuses `settings_save`.
- **Defaults, not `(unset)`:** unlike RFC-023's optional `[github]`/`[coordination]` sections that render `(unset)`, the `[tui]` surface always has concrete buffer values (`UiConfig`/`StatusBarConfig`/`MultiLineConfig` all `#[serde(default)]` / `Default`), so Interface rows always show a real default (`false` / `5` / `true` / each zone's RFC-022 default name list) rather than blank.
