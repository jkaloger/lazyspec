---
title: Remove the rules editor from the TUI settings panel
type: iteration
status: draft
author: Jack Kaloger
date: 2026-09-01
tags: []
related:
- implements: STORY-259
- blocks: ITERATION-384
- blocks: ITERATION-386
---

## Objective

The settings panel's "Validation Rules" category, and everything that exists to edit a `ValidationRule` through it -- `RuleKey`, `FieldPath::Rule`, the field readers and writers, the shape cycler, and add/delete -- are removed, so the TUI can no longer author a config shape the load path is about to refuse.

## Satisfies

STORY-259 AC3, in part: the TUI surface -- 22 `ValidationRule` sites across `src/tui/state/app.rs`, `src/tui/state/forms.rs` and `src/tui/views/panels.rs`. It is separated from ITERATION-385 because it is the largest single-file cluster on the story and because it must land *before* the load path refuses `[[rules]]`, not after. AC1, AC2 land in ITERATION-384; AC4 in ITERATION-383; AC5 in ITERATION-385.

## Context

- Story + ACs: STORY-259
- Deletion rather than deprecation, and why the load path carries no silent tolerance: STORY-259 §Notes; ADR-011 §Decision
- Touch:
  - `src/tui/state/forms.rs` -- `RuleKey` (`:611-619`) and `FieldPath::Rule` (`:642-645`). Every `match` on `FieldPath` in the TUI is exhaustive by design (`forms.rs:622-628` says so), so removing the variant is what surfaces the full call graph
  - `src/tui/state/app.rs` -- `settings_categories()` (`:987-999`) drops `"Validation Rules"`; `rule_raw` (`:187-209`), `rule_write` (`:211-241`), `settings_cycle_rule_shape` (`:1518-1543`), the field-read arm (`:1053-1057`), the field-write arm (`:1169-1173`), the shape special case (`:1418-1435`), the severity arm (`:1463-1467`), the add-entry arm (`:2723-2731`), the delete-confirm name lookup (`:2828-2834`) and the delete arm (`:2885-2888`)
  - `src/tui/views/panels.rs` -- the drilled-entry field list (`:2237-2325`) and the entry-name list (`:2575-2580`)
  - Tests: `app.rs:6176` `ac7_rule_severity_cycles`, `:6207` `ac7_rule_shape_converts_variant_preserving_name_and_severity`, and `panels.rs:4100`/`:4111`/`:4127` (`settings_lines_validation_rules_*`)
- **The cost this slice has to accept.** `settings_categories()` is a positional list and the TUI tests address categories by index -- `settings_app(config, 3, 4)` means "Validation Rules, severity" (`app.rs:6185`). Removing entry 3 shifts Numbering, GitHub, Certification, Agents and Interface down one, so every index-addressed settings test in `app.rs` and `panels.rs` needs re-basing, including ones with nothing to do with rules. Re-base them; do not leave a placeholder category to hold the index still. STORY-260 adds an "Edges" category, most likely back at that position, and a placeholder would make that story's diff lie about what changed.
- Between this slice and STORY-260 the TUI can read the DAG but not edit it. That is a real gap and it is the reason this slice exists as its own -- see Out of scope.
- `app.rs:1588-1640` re-parses the buffer through `Config::parse` before any write, and maps a load failure back to the offending field. Once `[[rules]]` is refused (ITERATION-384), a buffer still holding rules would make that guard reject the user's own save with a message pointing at a field the panel no longer shows. Landing this slice first is what prevents that.

## Tasks

1. Test-first, by deletion and re-basing: convert `settings_lines_validation_rules_not_drilled_shows_entries` into an assertion that `settings_categories()` no longer offers a rules category, and re-base the index-addressed settings tests onto the shortened list.
2. Remove `FieldPath::Rule` and `RuleKey`. Compile, and let the exhaustive matches enumerate the rest of the work -- that is the point of the design note at `forms.rs:622-628`; do not pre-emptively hunt call sites by grep.
3. Delete `rule_raw`, `rule_write` and `settings_cycle_rule_shape`, and the add/delete/severity/shape arms that reach them. `settings_cycle_rule_shape` is the only variant-converting mutator in the panel; check nothing else routes through it before deleting.
4. Drop `"Validation Rules"` from `settings_categories()` and the rules branches from `panels.rs`'s drilled-field and entry-name builders.
5. Confirm the save guard: with the category gone, a settings save on a *scratch* config that still declares `[[rules]]` round-trips them untouched -- `write_config_in_place` preserves what the buffer holds. Assert that, so the slice is provably read-only about rules rather than accidentally destructive. (This repo's own config no longer declares any, per ITERATION-380.)

## Out of scope

- An "Edges" settings category -> STORY-260. This slice deliberately leaves the panel unable to edit the DAG. Do not part-build the replacement here: STORY-260 owns the whole category, and half of it landing under a rules-removal slice would leave neither story reviewable.
- `Config.rules`, `ValidationRule` and `write_rules` -> ITERATION-385. After this slice the buffer still carries the field; nothing in the TUI reads or writes it.
- Refusing `[[rules]]` at load -> ITERATION-384.
- `config add-gate` and the create gate -> ITERATION-381.

## Principles / conventions

`lazyspec convention` and the dictums it lists. The TUI dictums: state and rendering stay separate, so `app.rs` loses the transitions and `panels.rs` loses the rendering, in that order. Overlays are state variants -- the delete-confirm path is one, and it must lose its rules arm rather than gain a fallthrough.

## Verification

`cargo run -- --help` aside, open the TUI settings screen: nine categories become eight, `Numbering` is where `Validation Rules` was, and the settings save on this repo leaves `.lazyspec.toml` byte-identical.
