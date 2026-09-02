---
title: Choose target types one at a time, wildcard included
type: iteration
status: in-progress
author: Jack Kaloger
date: 2026-09-01
tags: []
related:
- implements: STORY-260
---

## Objective

The `to` field opens a two-pane picker that adds and removes individual target types and offers `"*"` alongside every declared type name; `from` uses the same picker; the interim comma editor for both is gone.

## Satisfies

STORY-260 AC3. AC1, AC2, AC5 landed in ITERATION-386, ITERATION-387 and ITERATION-388; AC4 in ITERATION-389 and AC6 in ITERATION-390. Last slice on the story.

## Context

- Story + ACs: STORY-260
- `to` is a type selector so "an iteration implements a spike, a story, or a bug" is one row: ADR-030 §Decision. `"*"` on any position: ADR-031 §Decision
- `required` on a set means "any one member", not one edge per member: RFC-067 §Design. The picker must not imply otherwise in its labelling
- Touch:
  - `src/tui/state/forms.rs:440-446` `FieldEditor::ZoneOrdering` and `:451-620` `ZoneOrderingEditor` -- the existing two-pane Selected/Available editor with `toggle_pane`, `add`, `remove`, `move_up`, `move_down`
  - `src/tui/state/app.rs:1256-1262` (`settings_start_edit` routing), `:1283-1295` `settings_open_zone_editor`, `:1300-1320` `settings_commit_zone` / `settings_cancel_zone`, `:970-984` the key-context precedence
  - `src/tui/views/keys.rs:865-...` -- the editor owns all keys while open; `c` commits, `Esc` cancels
  - `src/tui/views/overlays.rs:385` `draw_settings_zone_editor`
  - `src/tui/state/app.rs:8055-8140` -- the existing status-bar zone editor tests, and `:8215` `dummy_zone_editor`
- **Why this is not a straight reuse.** Three things are hardcoded to the status bar. `ZoneOrderingEditor::available_for` (`forms.rs:493-500`) reads `STATUS_BAR_COMPONENTS` directly; `settings_open_zone_editor` matches the three statusbar `FieldPath`s and returns early on anything else; `settings_commit_zone` writes into `ui.statusbar`. All three need the vocabulary and the commit target to come from the path. That generalisation is the work of this slice, and dictum 6 sanctions it: this is the second concrete use, not a speculative one.
- **A type set is unordered.** `ZoneOrderingEditor` exists to *order* -- `move_up` / `move_down` at `forms.rs:565-585` -- and a `to` set has no order that means anything: `TypeSelector::Types(Vec<String>)` is a set spelled as a list and `EdgeDef::matches` (`config.rs:67-78`) is order-blind. Reusing the editor as-is hands the user two keys that dirty the file and change no behaviour. Decide: suppress reordering for a type-set path, or add a `FieldEditor` variant sharing the two-pane state without the ordering keys. A whole parallel overlay is the wrong answer either way.
- **`"*"` is a variant, not a member.** `TypeSelector` is `Any | Types(Vec<String>)` (`config.rs:98-121`), so picking `*` is not "add `*` to the selected list" -- it is a different variant. Picking `*` must clear the concrete members, and adding a concrete member must clear `*`. AC3's "`\"*\"` is offered as a choice alongside declared type names" is one radio dressed as a checkbox, and the picker has to enforce the exclusivity rather than let the commit silently discard half the selection. Do not let `Types(vec!["*"])` become representable: the parser reads a scalar `"*"` as `Any` (`config.rs:1765` `edge_to_reads_a_scalar_as_a_single_element_list` and the wildcard tests at `:1833`), so a list containing `"*"` is a shape only the panel could produce.
- **The empty selection.** `Types(vec![])` passes strict load -- the check at `config.rs:1327-1336` iterates `names()` and an empty list iterates nothing -- and means nothing. ITERATION-387 refused it at commit for the comma editor; carry the same refusal here, with the same message, for the same reason. Two refusals with two wordings is the drift STORY-260 §Notes is about, one layer down from AC4.
- This slice removes the interim `FieldEditor::List` from `from` and `to`. Leaving both spellings live is the failure mode, not a fallback.

## Tasks

1. Test-first through the App state API, no terminal: open the picker on an edge's `to`, add two types, remove one, commit, and assert `edges[0].to == Types(vec![..])`; reopen, pick `*`, commit, and assert `Any` with the concrete members gone; reopen, add a type, and assert the `*` is gone.
2. Generalise the vocabulary: `available_for` takes the vocabulary rather than reading the status-bar const, and the type-set path supplies `"*"` plus `settings_buffer.documents.types` names. The existing status-bar tests at `app.rs:8055-8140` are the regression guard that the generalisation changed nothing there -- run them before touching the overlay, not after.
3. Generalise open and commit: both dispatch on `FieldPath`, adding `FieldPath::Edge { key: From | To }`.
4. Resolve the ordering question from Context, and remove whatever keys the answer makes meaningless, in `keys.rs` and in the overlay's rendered help line -- not just in the state.
5. Point `from` and `to` at the picker in `panels.rs` and delete the interim `List` editor for both. Carry ITERATION-387's empty-set refusal across verbatim.
6. `README.md:190-206`: the settings-view key table describes `Enter` as drilling into an entry or starting a field edit, and never mentions the two-pane editor -- the status-bar zone editor is already undocumented there. Document the picker and its keys (`Tab`, `c`, `Esc`), which covers both, since this slice is what makes the omission bite.

## Out of scope

- Ordering within a type set, if the Context decision keeps the ordering keys for the status bar only. A `to` list's order is not meaningful and the file should not record one the user thinks matters.
- Offering a type name the config does not declare. The vocabulary is the buffer's `[[types]]` plus `"*"`; anything else is the load error ITERATION-389 surfaces, and a picker that can produce it is a worse picker.
- `via`'s vocabulary -- ITERATION-387 settled whether it is a cycler over declared relationship names or a text field. Do not reopen it here just because a picker now exists.
- Enforcing unique edge names or a non-empty target set at load -- both holes are recorded in ITERATION-388 and ITERATION-390 and neither has an AC.
- `[[edges]]` in the `config` CLI and the `init` wizard -> STORY-261.

## Principles / conventions

`lazyspec convention` and the dictums it lists. TUI dictums: overlays are state variants, not separate widget trees, so the picker is the existing overlay generalised and the view checks which is active. Dictum 6: two concrete uses justify the indirection; a third editor would not.

## Verification

Drill into this repo's `general-relatedness` edge, open `to`: the Available pane offers `*` plus every declared type name. Select two types, commit, save, and `git diff .lazyspec.toml` shows only that row's `to`. Reopen, pick `*`, save: the row reads `to = "*"` and not `to = ["*"]`.
