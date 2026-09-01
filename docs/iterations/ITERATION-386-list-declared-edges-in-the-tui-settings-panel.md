---
title: List declared edges in the TUI settings panel
type: iteration
status: accepted
author: Jack Kaloger
date: 2026-09-01
tags: []
related:
- implements: STORY-260
- blocks: ITERATION-387
---

## Objective

An "Edges" settings category lists every `[[edges]]` row by `name` with its `from`, `via` and target set on the row, and drilling into a row shows its keys as read-only fields. `FieldPath::Edge` and `EdgeKey` exist; nothing writes through them yet.

## Satisfies

STORY-260 AC1. Separated from AC2 because the category's arrival is ITERATION-382's positional churn played backwards -- the 32 index-addressed `settings_app(` call sites in `app.rs` re-base a second time -- and because the read path and the write path are two exhaustive matches, each a session's work. AC2 lands in ITERATION-387, AC5 in ITERATION-388, AC4 in ITERATION-389, AC6 in ITERATION-390, AC3 in ITERATION-391.

## Context

- Story + ACs: STORY-260
- What an edge row is and what each position means: ADR-030 §Decision; RFC-067 §"Interface sketch"
- `"*"` on any of the three positions: ADR-031 §Decision
- Touch:
  - `src/tui/state/forms.rs:606-609` -- `EdgeKey` goes beside `RelKey`, and `FieldPath::Edge { index, key }` where `FieldPath::Rule` was before ITERATION-382 removed it. The `FieldPath` doc comment (`:622-628`) states every match on it is exhaustive by design; that is what makes the compiler enumerate this slice's call graph rather than a grep
  - `src/tui/state/app.rs:987-999` -- `settings_categories()`. `"Edges"` goes back at index 3, the slot ITERATION-382 vacated and named this story as the refiller of
  - `src/tui/state/app.rs:1018` `settings_focused_raw` and the `settings_write` match at `:1160-1250` -- the single exhaustive read/write pair. The write arm lands here as a documented no-op; a `FieldPath` variant with no write arm does not compile
  - `src/tui/views/panels.rs:2101` `settings_fields` -- the `3 =>` arm, whose rules body ITERATION-382 deleted, gains the edge field list
  - `src/tui/views/panels.rs:2561` `settings_entry_names` and `:2643` `drill_entry_name`
  - `src/engine/config.rs:55-65` `EdgeDef`; `:98-135` `TypeSelector` / `RelSelector`. The displayed value must be the TOML spelling the user wrote; `config.rs:1958` `edge_selectors_re_emit_the_toml_spelling_they_were_written_in` pins that contract for `to_toml`, and the panel is a second renderer of the same thing
  - Tests: every `settings_app(` call site in `app.rs`, plus `panels.rs:3975` `settings_entry_names_source_from_model_without_prefix`, `:4305` `settings_fields_entry_list_views_are_empty`, `:4323` `settings_fields_and_lines_agree_for_field_view`
- **The decision this slice has to make.** `settings_entry_names` returns one string per entry and three consumers read it: the entry-list render, `drill_entry_name` (the drilled-view title) and the `#[cfg(test)]` line builder at `panels.rs:2596`. AC1 wants the triple visible *in the list*, so an edge's entry string is not a bare `name`. Compose it -- `name`, then `from -via-> to` -- and leave `drill_entry_name` on the bare `name`, which means that function grows a display/identity split rather than being reused as-is. The alternative reads AC1's "listed by `name` with their `from`, `via`, and target set visible" as two screens, and it is one.
- **What the read-only interlude costs.** Between this slice and the next, the panel shows the DAG and refuses to change it -- worse than ITERATION-382's state only in that it now looks editable. Keep every edge field `FieldEditor::ReadOnly` (`forms.rs:445`), not `Text` with a dead write arm, so `settings_start_edit` (`app.rs:1256-1277`) declines instead of opening an editor whose commit does nothing.
- `settings_fields`' `3 =>` arm cannot borrow the rules pattern wholesale: a rule had one shape per variant, an edge has one shape, so the arm is a flat push with no `match`. `RULE_SHAPE_VARIANTS` (`panels.rs:2065`) went with ITERATION-382; a table with one shape does not get a variant list back.
- The web view has no settings surface -- `src/web/` renders documents (`render.rs`, `routes.rs`) and never writes config -- so the project's TUI/CLI/web parity instruction resolves here to TUI now and CLI in STORY-261. Recorded so a reviewer does not go looking.

## Tasks

1. Test-first in `panels.rs`: `settings_entry_names(3, &config)` on a config with two `[[edges]]` returns one line per row carrying `name`, `from`, `via` and the target set, with `Any` spelled `*`; and `settings_fields(3, _, Some(0), &config)` returns the row's keys in a fixed order, every one `ReadOnly`.
2. Add `EdgeKey` and `FieldPath::Edge` to `forms.rs`. Compile, and let the exhaustive matches list the sites.
3. Put `"Edges"` at index 3 in `settings_categories()` and re-base every index-addressed settings test in `app.rs`. The shift is silent -- a stale index addresses a real category and the test still passes -- so wherever a test is touched anyway, assert on the category name or the resolved `FieldPath`, not on the index alone.
4. Build the drilled field list and the entry-list line in `panels.rs`, both deriving from `EdgeDef`. One helper renders a `TypeSelector`; the entry line and the `to` field row both call it.
5. Add the read arm to `settings_focused_raw` and the no-op write arm to `settings_write`, the latter commented with the iteration that fills it.
6. `panels.rs:4305` `settings_fields_entry_list_views_are_empty` asserts the not-drilled view has no fields. Confirm it still holds for category 3 rather than assuming: an entry list is entry names, and the field list stays empty.

## Out of scope

- Editing any edge field (AC2) -> ITERATION-387. This slice deliberately ships a read-only category.
- The target-set picker and `"*"` as an offered choice (AC3) -> ITERATION-391.
- Writing edges to disk (AC5) -> ITERATION-388. Nothing this slice does can reach `.lazyspec.toml`: `write_config_in_place` (`src/engine/config_write.rs:10-27`) has no edge writer at all.
- Rejecting a bad edit (AC4) -> ITERATION-389; seeding a row (AC6) -> ITERATION-390. `n` and `d` on this category do nothing until then, because `settings_seed_entry` (`app.rs:2683`) and `settings_open_delete_confirm` (`app.rs:2795`) lost their `3 =>` arms to ITERATION-382 and this slice does not add them back.
- `[[edges]]` in the `config` CLI and `init` -> STORY-261.

## Principles / conventions

`lazyspec convention` and the dictums it lists. The TUI dictums: views read state and never mutate it, so `panels.rs` gains rendering and `app.rs` gains only a reader. Dictum 6: one selector renderer, not one per call site.

## Verification

Open the TUI settings screen: eight categories become nine, `Edges` sits where `Validation Rules` used to, and this repo's edge rows list with their triples. `Enter` on an edge field opens nothing. `w` on an untouched buffer leaves `.lazyspec.toml` byte-identical.
