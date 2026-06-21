---
title: mode-aware help overlay from a keybind SSOT
type: iteration
status: accepted
author: agent
date: 2026-06-20
tags:
- tui
- keybindings
- help
related:
- related-to: RFC-011
---

## Problem

`draw_help_overlay` (`src/tui/views/overlays.rs:15`) = static hand-list. Drift
since `#27` restructure. Mode-blind: one fixed list regardless of `view_mode`.
Omits Filters/Graph/Settings/Agents/Search + every dialog keyset. Also `?` only
opens help in Types (`keys.rs:1016`) + Filters (`keys.rs:610`); Graph/Settings/
Agents/Fullscreen never open help. Root cause: help authored apart from
dispatch, no single source of truth.

## Decisions

- Help = **mode-aware**. `?` shows only keys live in current screen/dialog.
- Drift guard = **one keybind registry**. Help render + parity test both consume
  it. Test fails if dispatch and registry disagree.

## Changes

1. **New module `src/tui/views/keybinds.rs`** (registry = SSOT). AC: registry.
   - `enum KeyContext` — one variant per active handler in `handle_key` ladder
     (`keys.rs:19-66`): `Warnings`, `CreateForm`, `DeleteConfirm`,
     `OverrideKeyPrompt`, `SettingsDeleteConfirm`, `SettingsImpact`,
     `StatusPicker`, `LinkEditor`, `ProvenanceEditor`, `AgentDialog` (cfg
     `agent`), `AgentTextInput` (cfg `agent`), `Search`, `Fullscreen`, +
     ViewModes `Types`/`Filters`/`Graph`/`Agents`(cfg `agent`), + Settings
     sub-states `Settings`, `SettingsEditing`, `SettingsQuitPrompt`,
     `SettingsZoneEditor`, `SettingsVariantPicker`, `SettingsScaffoldOffer`,
     + `GhConflict`. Feature-gate agent variants with `#[cfg(feature="agent")]`.
   - `struct Keybind { keys: &'static str, desc: &'static str, char_catchall: bool }`.
     `keys` = display (e.g. `"j/k"`). `char_catchall=true` marks a `Char(c) =>`
     type-text arm (collapses all printable chars to one entry for parity).
   - `struct KeybindGroup { title: &'static str, binds: Vec<Keybind> }`.
   - `fn keybinds_for(ctx: KeyContext) -> Vec<KeybindGroup>`. Honest per-context:
     ONLY keys that handler acts on (dialogs swallow `` ` ``/`5`/`q`). Source the
     per-context keymap from `## Notes` below. Each binding maps to a `KeyChord`
     (code + ctrl flag) used by the parity test.
   - `fn context_label(ctx) -> &'static str` for the popup title.

2. **`App::active_key_context(&self) -> KeyContext`** in `src/tui/state/app.rs`.
   AC: mode-aware content. Mirror `handle_key` precedence EXACTLY (`keys.rs:
   19-66`), skipping the `show_help` short-circuit. Second consumer of same
   ladder => structural drift guard. Resolve Settings sub-state in same order as
   `handle_settings_key` (`keys.rs:744-885`): quit_prompt > editing >
   zone_editor > variant_picker > scaffold_offer > nav. State query lives in
   `tui/state` (DICTUM-007: views read state, not compute it).

3. **Mode-aware render** `draw_help_overlay(f, app: &App)` in
   `src/tui/views/overlays.rs:15`. AC: mode-aware, popup-fit/scroll.
   - Compute `app.active_key_context()`, render `keybinds_for(ctx)` groups.
   - Title `format!(" Help — {} ", context_label(ctx))`.
   - Height = content rows capped at `area.height - 4`. On overflow scroll by
     `app.help_scroll`.
   - Delete the static `help_text` vec + `#[cfg(feature="agent")] insert(11,..)`.
   - Add `pub help_scroll: u16` to `App` (init 0 at both ctors `app.rs:615`,
     `:2897`); reset to 0 when `?` sets `show_help=true`.
   - Update all 4 call sites `draw_help_overlay(f)` -> `(f, app)` in
     `src/tui/views.rs` (lines 70, 89, 108, 268).

4. **Open help everywhere reachable** in `src/tui/views/keys.rs`. AC: `?` opens
   help in all non-text contexts.
   - Add `KeyCode::Char('?') => self.show_help = true; self.help_scroll = 0;` to
     `handle_graph_key`, `handle_agents_key`, `handle_fullscreen_key`, and
     `handle_settings_key` NAV arm only (NOT editing/zone/variant/quit-prompt/
     scaffold sub-states — there `?` stays inert or literal).
   - Update existing Types/Filters `?` arms to also reset `help_scroll`.
   - Help-open dismiss (`keys.rs:25-28`): keep "any key closes", EXCEPT when the
     active context's help overflowed — then `j`/`k`/`↓`/`↑` scroll
     (`help_scroll`) and any other key dismisses. Gate scroll on an overflow
     check so non-overflow help keeps pure any-key dismiss (AC6).
   - Do NOT bind `?` in text-input handlers (search, create_form, settings
     editing, link/provenance/agent text, override prompt): `?` is literal there.

5. **Parity test** `#[cfg(test)] mod tests` in `keybinds.rs`. AC: parity both
   directions; old static list removed.
   - Build fresh `App` per case via `App::new(store, &config, Picker::
     halfblocks(), Box::new(RealFileSystem))` (pattern: `event_loop.rs:762`),
     seeded into each `KeyContext` (set the active flag / view_mode / sub-state).
   - `fn fingerprint(&App) -> impl PartialEq` over salient fields (`view_mode`,
     `selected_doc`/`selected_type`/`settings_*` indices, every `*_request`
     `Option::is_some`, `should_quit`, `show_help`, each dialog `active` bool,
     `settings_dirty`, buffers' len). App is NOT `Clone` => snapshot via this
     fn, not clone.
   - Candidate keys: `a-z A-Z 0-9`, punctuation used by handlers
     (`/ ? \` space backtick`), `Enter Esc Tab BackTab Space Up Down Left Right
     Backspace`, each with + without `CONTROL`. For each: fresh seeded app,
     `handle_key`, "handled" = fingerprint changed.
   - Collapse: if context registry has a `char_catchall` bind, bucket all
     printable `Char(_)` presses under one token before comparing.
   - Assert per context: handled-set == registry-chord-set. Registry⊇dispatch
     catches undocumented keys; dispatch⊇registry catches dead help rows.
   - Skip seeding combos that need real docs where infeasible; document any
     context the test cannot seed (no silent gaps).

## Test Plan

Tests are state-level (DICTUM-007: TUI state testable without a terminal).

- **AC `?` opens help everywhere**: per non-text context, seed app, press `?`,
  assert `show_help == true`. Negative: in a text-input context (e.g. search),
  press `?`, assert `show_help == false` AND `?` landed in the input buffer.
  Property: behavioral (observes state), specific (one context per case).

- **AC mode-aware content**: for each `view_mode`/dialog, assert
  `active_key_context()` returns the matching variant. Assert `keybinds_for(ctx)`
  is non-empty and excludes a key known to belong only to another context
  (e.g. Graph help has no `x` wrap, Settings help has no `/`).

- **AC parity both directions** (the load-bearing test, task 5): per context,
  handled-set == registry-set. Tradeoff: fingerprint coverage. A field omitted
  from `fingerprint` => a real keypress reads as no-op => false parity pass.
  Mitigation: fingerprint covers all mutation sinks listed in task 5; review
  against each handler's writes. This is the isolated/deterministic vs
  exhaustive tradeoff — favour exhaustive sinks over brevity.

- **AC single source / static list removed**: grep-style assertion not needed;
  covered by deleting `help_text` and render reading `keybinds_for`. Render smoke
  test: build app in Types, render to a `TestBackend` buffer, assert a Types key
  string present and a Settings-only key string absent.

- **AC any-key dismiss + scroll**: non-overflow context — press arbitrary key,
  assert `show_help=false`. Overflow context (seed many binds / tiny area) —
  press `j`, assert `help_scroll` increments and `show_help` stays true; press
  `x`, assert dismiss.

## Notes

### Full per-context keymap (from `keys.rs`, source of truth for the registry)

Ladder order = precedence in `handle_key` (`keys.rs:19-66`). `[TEXT]` = has
`Char(c)` catch-all (char_catchall).

- `GhConflict` (`:19`): `Esc` close.
- `Warnings` (`handle_warnings_key:192`): `Esc`/`w`/`q` close, `f` fix, `j`/`↓`
  down, `k`/`↑` up.
- `CreateForm` (`:69`) [TEXT]: `Esc` cancel, `Enter` submit, `Tab` next field,
  `BackTab` prev, `Backspace`, `<any>` type.
- `DeleteConfirm` (`:89`): `Enter` confirm, `Esc` cancel.
- `OverrideKeyPrompt` (`:115`) [TEXT]: `Enter`, `Esc`, `Backspace`, `<any>` type.
- `SettingsDeleteConfirm` (`:99`): `Enter`, `Esc`.
- `SettingsImpact` (`:107`): `Enter`/`y` confirm, `Esc`/`n` cancel.
- `StatusPicker` (`:125`): `j`/`↓`, `k`/`↑`, `Enter`, `Esc`.
- `LinkEditor` (`:145`) [TEXT]: `Esc`, `Tab` cycle rel-type, `Enter`, `j`/`↓`,
  `k`/`↑`, `Backspace`, `<any>` type.
- `ProvenanceEditor` (`:180`) [TEXT]: `Esc`, `Enter`, `Backspace`, `<any>` type.
- `AgentDialog` (cfg agent, `:269`): `Esc`, `Up`, `Down`, `Enter`.
- `AgentTextInput` (cfg agent, `:387`) [TEXT]: `Esc`, `Enter`, `Backspace`,
  `<any>` type.
- `Search` (`:431`) [TEXT]: `Esc`, `Enter`, `Backspace`, `↑`, `↓`, `Ctrl-k` up,
  `Ctrl-j` down, `<any>` type.
- `Fullscreen` (`:455`): `Esc`/`q` exit, `j`/`↓` down, `k`/`↑` up, `g` top,
  `G` bottom, `Ctrl-d` half-down, `Ctrl-u` half-up. (+`?` after task 4)
- `Filters` (`:531`): `Ctrl-d`/`Ctrl-u` half page; `Tab`/`BackTab` focus;
  `h`/`←` `l`/`→` cycle value; `Enter` clear/relation/open; `j`/`↓` `k`/`↑`;
  `g`/`G`; `e` edit; `q`; `` ` `` cycle; `5` settings; `?` help; `/` search;
  `w` warnings; `s` status; `r` relation; `p` provenance.
- `Graph` (`:632`): `j`/`↓` `k`/`↑`; `Enter` open; `g`/`G`; `e` edit; `q`;
  `` ` `` cycle; `5` settings. (+`?` after task 4)
- `Settings` nav (`:894`): `j`/`↓` `k`/`↑` field/entry; `l`/`→` `h`/`←` category;
  `n` new (if !drill); `d` delete (cond); `Enter` edit/drill; `Esc` back/quit;
  `q` quit/quit-prompt; `` ` `` cycle; `5` noop; `w`/`Ctrl-s` save. (+`?` task 4)
- `SettingsEditing` (`:776`) [TEXT]: `Esc`, `Enter`, `Backspace`, `<any>` type.
- `SettingsQuitPrompt` (`:744`): `s` save, `d` discard, `Esc` cancel.
- `SettingsZoneEditor` (`:795`): `Tab` pane, `j`/`↓` `k`/`↑`, `Space`/`Enter`
  add/remove, `K` move up, `J` move down, `c` commit, `Esc` cancel.
- `SettingsVariantPicker` (`:841`): `j`/`↓` `k`/`↑`, `Enter`, `Esc`.
- `SettingsScaffoldOffer` (`:875`): `g` jump to field, `<other>` declines.
- `Agents` (cfg agent, `:476`): `Ctrl-d`/`Ctrl-u` half; `j`/`↓` `k`/`↑`;
  `e` edit; `r` resume; `q`; `` ` `` cycle; `5` settings. (+`?` after task 4)
- `Types` default (`:996`): `q`/`Ctrl-c` quit; `?` help; `/` search; `n` new;
  `Ctrl-d`/`Ctrl-u` half; `d` delete; `e` edit; `x` wrap; `Enter` relation/
  fullscreen; `j`/`↓` `k`/`↑`; `l`/`→` `h`/`←` type; `Space` expand/collapse;
  `Tab` preview tab; `g`/`G`; `` ` `` cycle; `5` settings; `w` warnings;
  `s` status; `p` provenance; `r` relation; `R` reload; `a` agent (cfg agent).

### Risks

- Parity fingerprint must cover EVERY mutation sink or it false-passes. Highest
  risk in the iteration; review fingerprint against each handler's writes.
- `?`/AC6 tension: scroll needs `j`/`k` while help open vs "any key dismiss".
  Resolved: scroll only when overflow, else any-key dismiss.
- Help unreachable in text-input contexts by design (`?` is literal). Registry
  still documents them; parity test still covers them. Acceptable: AC for `?`
  scopes to non-text contexts.
